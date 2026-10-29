"""OpenCode/OpenAI contracts: auth ownership, quota windows, feed health, routing."""

import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import urllib.error

from matrixd import daemon, render
from matrixd.sources import claude_session, opencode_openai as quotas, opencode_session as sessions, usage


def quota_payload(five=42, seven=17):
    return {"rate_limit": {
        "primary_window": {"used_percent": five, "limit_window_seconds": 18000, "reset_at": 1800000000},
        "secondary_window": {"used_percent": seven, "limit_window_seconds": 604800},
    }}


class TestQuotas(unittest.TestCase):
    def test_duration_not_position_controls_the_bars(self):
        payload = quota_payload()
        a, b = payload["rate_limit"].values()
        payload["rate_limit"] = {"primary_window": b, "secondary_window": a}
        result = quotas.parse(payload, 10)
        self.assertEqual(result.five_hour.percent, 42)
        self.assertEqual(result.seven_day.percent, 17)
        self.assertEqual(result.five_hour.resets_at.timestamp(), 1800000000)
        self.assertFalse(result.is_stale(310))
        self.assertTrue(result.is_stale(311))

    def test_unknown_duration_does_not_mislabel_an_allowance(self):
        payload = quota_payload()
        payload["rate_limit"]["primary_window"]["limit_window_seconds"] = 3600
        result = quotas.parse(payload, 0)
        self.assertIsNone(result.five_hour)
        self.assertEqual(result.seven_day.percent, 17)

    def test_bad_windows_do_not_erase_good_ones_or_crash(self):
        for value in (None, "50", True, float("nan"), float("inf"), -1, 101, 10**400):
            with self.subTest(value=value):
                result = quotas.parse(quota_payload(five=value), 0)
                self.assertIsNone(result.five_hour)
                self.assertEqual(result.seven_day.percent, 17)
        for value in (None, [], 2, {}, {"rate_limit": []}, {"rate_limit": {"primary_window": []}}):
            self.assertIsNone(quotas.parse(value, 0))

    def test_credits_and_model_specific_buckets_are_not_generic_windows(self):
        self.assertIsNone(quotas.parse({"credits": {"balance": "20"},
                                      "additional_rate_limits": [quota_payload()]}, 0))

    def test_reset_time_can_fail_without_losing_the_percentage(self):
        for value in ("tomorrow", float("inf"), 10**400, True):
            payload = quota_payload()
            payload["rate_limit"]["primary_window"]["reset_at"] = value
            result = quotas.parse(payload, 0)
            self.assertIsNone(result.five_hour.resets_at)
            self.assertEqual(result.five_hour.percent, 42)

    def test_request_uses_opencode_access_and_account_without_refresh(self):
        credentials = quotas.Credentials("test-access", "test-account")
        with patch.object(quotas.urllib.request, "urlopen", return_value=io.StringIO(json.dumps(quota_payload()))) as request:
            result = quotas.fetch(10, credentials)
        req = request.call_args.args[0]
        self.assertEqual(req.full_url, quotas.USAGE_URL)
        self.assertEqual(req.get_method(), "GET")
        self.assertEqual(req.get_header("Authorization"), "Bearer test-access")
        self.assertEqual(req.get_header("Chatgpt-account-id"), "test-account")
        self.assertEqual(request.call_args.kwargs["timeout"], usage.REQUEST_TIMEOUT)
        self.assertEqual(result.five_hour.percent, 42)
        self.assertNotIn("test-access", repr(credentials))

    def test_http_auth_network_and_decode_failures_return_unknown(self):
        for error in (urllib.error.HTTPError(quotas.USAGE_URL, 401, "expired", {}, None),
                      urllib.error.HTTPError(quotas.USAGE_URL, 429, "limited", {}, None),
                      urllib.error.URLError("offline"), TimeoutError()):
            with patch.object(quotas.urllib.request, "urlopen", side_effect=error):
                self.assertIsNone(quotas.fetch(0, quotas.Credentials("test")))
        with patch.object(quotas.urllib.request, "urlopen", return_value=io.StringIO("not json")):
            self.assertIsNone(quotas.fetch(0, quotas.Credentials("test")))


class TestCredentials(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        env = patch.dict(os.environ, {"XDG_DATA_HOME": self.temp.name}, clear=True)
        env.start()
        self.addCleanup(env.stop)
        self.path = Path(self.temp.name, "opencode", "auth.json")
        self.path.parent.mkdir()

    def test_xdg_file_is_read_only_and_reread(self):
        for access in ("first", "rotated"):
            text = json.dumps({"openai": {"type": "oauth", "access": access, "refresh": "untouched", "accountId": "account"}})
            self.path.write_text(text)
            self.assertEqual(quotas.read_credentials(), quotas.Credentials(access, "account"))
            self.assertEqual(self.path.read_text(), text)

    def test_environment_auth_has_precedence(self):
        self.path.write_text('{"openai":{"type":"oauth","access":"file"}}')
        with patch.dict(os.environ, {"OPENCODE_AUTH_CONTENT": '{"openai":{"type":"oauth","access":"environment"}}'}):
            self.assertEqual(quotas.read_credentials().access, "environment")
        with patch.dict(os.environ, {"OPENCODE_AUTH_CONTENT": "broken"}):
            self.assertIsNone(quotas.read_credentials())

    def test_missing_malformed_api_key_and_invalid_headers_are_rejected(self):
        self.assertIsNone(quotas.read_credentials())
        for value in (None, [], {}, {"openai": None}, {"openai": {"type": "api", "key": "api-key"}},
                      {"openai": {"type": "oauth", "access": "bad\nheader"}},
                      {"openai": {"type": "oauth", "access": "ok", "accountId": []}}):
            self.path.write_text(json.dumps(value))
            self.assertIsNone(quotas.read_credentials())


class TestQuotaCache(unittest.TestCase):
    def test_same_account_failure_retains_data_for_at_most_five_minutes(self):
        source = quotas.Source()
        result = quotas.parse(quota_payload(), 10)
        with patch.object(quotas, "read_credentials", return_value=quotas.Credentials("a", "account")), \
             patch.object(quotas, "fetch", side_effect=[result, None, None]):
            self.assertIs(source.fetch(10), result)
            self.assertIs(source.fetch(70), result)
            self.assertIsNone(source.fetch(311))

    def test_new_account_does_not_inherit_old_quota_on_failure(self):
        source = quotas.Source()
        with patch.object(quotas, "read_credentials", return_value=quotas.Credentials("a", "one")), \
             patch.object(quotas, "fetch", return_value=quotas.parse(quota_payload(), 10)):
            self.assertIsNotNone(source.fetch(10))
        for credentials in (quotas.Credentials("b", "two"), None):
            with patch.object(quotas, "read_credentials", return_value=credentials), \
                 patch.object(quotas, "fetch", return_value=None):
                self.assertIsNone(source.fetch(20))

    def test_account_switch_during_request_discards_response(self):
        with patch.object(quotas, "read_credentials", side_effect=[quotas.Credentials("a", "one"), quotas.Credentials("b", "two")]), \
             patch.object(quotas, "fetch", return_value=quotas.parse(quota_payload(), 10)):
            self.assertIsNone(quotas.Source().fetch(10))


class TestSessionFeed(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        env = patch.dict(os.environ, {"XDG_RUNTIME_DIR": self.temp.name})
        env.start()
        self.addCleanup(env.stop)
        self.directory = Path(sessions.snapshot_dir())
        self.directory.mkdir(parents=True)

    def snapshot(self, name, activity, heartbeat, **changes):
        session = dict(session_id=name, provider_id="openai", context_pct=42,
                       working=False, updated_at=activity)
        session.update(changes)
        path = self.directory / f"{name}.json"
        path.write_text(json.dumps({"version": 1, "sessions": [session]}))
        os.utime(path, (heartbeat, heartbeat))
        return path

    def test_heartbeats_do_not_steal_selection_from_meaningful_activity(self):
        self.snapshot("idle", 100, 1000)
        self.snapshot("active", 990, 995, working=True, context_pct=80)
        result = sessions.read(1000)
        self.assertEqual(result.session_id, "active")
        self.assertEqual(result.context_pct, 80)
        self.assertTrue(result.working)

    def test_idle_context_lives_with_heartbeat_but_expires_when_producer_dies(self):
        self.snapshot("idle", 10, 1000)
        self.assertIsNotNone(sessions.read(1060))
        self.assertIsNone(sessions.read(1061))

    def test_invalid_newest_does_not_hide_valid_session(self):
        self.snapshot("valid", 990, 1000)
        for change in ({"parent_id": "root"}, {"provider_id": "anthropic"},
                       {"updated_at": float("nan")}, {"updated_at": True},
                       {"updated_at": 10**400}):
            self.snapshot("bad", 999, 1000, **change)
            self.assertEqual(sessions.read(1000).session_id, "valid")
        (self.directory / "bad.json").write_text("{")
        self.assertEqual(sessions.read(1000).session_id, "valid")

    def test_bad_context_and_working_values_are_not_fabricated(self):
        self.snapshot("bad-values", 990, 1000, context_pct=True, working="working")
        result = sessions.read(1000)
        self.assertIsNone(result.context_pct)
        self.assertFalse(result.working)

    def test_pruning_only_removes_expired_opencode_producers(self):
        dead = self.snapshot("dead", 10, 10)
        live = self.snapshot("live", 20, 990)
        claude = Path(self.temp.name, "matrixd", "sessions")
        claude.mkdir()
        (claude / "keep.json").write_text("{}")
        self.assertEqual(sessions.prune(1000), 1)
        self.assertFalse(dead.exists())
        self.assertTrue(live.exists())
        self.assertTrue((claude / "keep.json").exists())


class TestProviderRouting(unittest.TestCase):
    def test_cli_default_and_explicit_provider(self):
        for argv, selected in (([], "claude"), (["--provider", "claude"], "claude"),
                               (["--provider", "opencode-openai"], "opencode-openai")):
            with patch.object(daemon, "Daemon") as factory:
                factory.return_value.run.return_value = 0
                self.assertEqual(daemon.main(argv), 0)
                factory.assert_called_once_with(provider=selected)
        with patch("sys.stderr", new=io.StringIO()), self.assertRaises(SystemExit) as error:
            daemon.main(["--provider", "openai"])
        self.assertEqual(error.exception.code, 2)

    def test_opencode_never_reads_or_prunes_claude_snapshots(self):
        with patch.object(daemon.audio, "read", return_value=None):
            d = daemon.Daemon("opencode-openai")
        with patch.object(sessions, "read", return_value=claude_session.Session("open", 42, True, 1000)) as read, \
             patch.object(sessions, "prune") as prune, \
             patch.object(claude_session, "read") as claude_read, \
             patch.object(claude_session, "prune") as claude_prune, \
             patch.object(d, "_start_usage_fetch"), patch.object(d, "reconcile_panels"):
            d._due["prune"] = 0
            d._run_due()
        read.assert_called_once()
        prune.assert_called_once()
        claude_read.assert_not_called()
        claude_prune.assert_not_called()
        self.assertEqual(d.state.session.session_id, "open")

    def test_five_minute_quota_expiry_keeps_session_and_layout(self):
        with patch.object(daemon.audio, "read", return_value=None):
            d = daemon.Daemon("opencode-openai")
        d.state.usage = quotas.parse(quota_payload(), 100)
        d.state.session = claude_session.Session("s", 42, True, 1000)
        with patch.object(daemon.time, "monotonic", return_value=401):
            frame = d.ambient_frames()["right"]
        self.assertEqual(frame, render.render_claude(render.ClaudeState(context_pct=42, working=True)))

    def test_selected_quota_worker_can_clear_account_cache(self):
        with patch.object(daemon.audio, "read", return_value=None):
            d = daemon.Daemon("opencode-openai")
        d.state.usage = quotas.parse(quota_payload(), 100)
        with patch.object(d.usage_source, "fetch", return_value=None) as fetch, patch.object(usage, "fetch") as claude_fetch:
            d._start_usage_fetch()
            d._usage_thread.join(2)
        self.assertFalse(d._usage_thread.is_alive())
        fetch.assert_called_once()
        claude_fetch.assert_not_called()
        self.assertIsNone(d.state.usage)

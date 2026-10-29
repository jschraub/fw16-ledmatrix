use matrix_client::Event;
use ts_rs::TS;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/desktop/src/bindings");
    let config = ts_rs::Config::new().with_out_dir(&path);
    Event::export_all(&config)?;
    matrix_client::Request::export_all(&config)?;
    matrix_client::Failure::export_all(&config)?;
    let failures: std::collections::BTreeMap<String, matrix_client::Failure> =
        matrix_client::ErrorCode::ALL
            .into_iter()
            .map(|code| {
                let key = serde_json::to_value(code)?
                    .as_str()
                    .expect("string error code")
                    .to_owned();
                Ok((key, matrix_client::Failure::new(code)))
            })
            .collect::<Result<_, serde_json::Error>>()?;
    std::fs::write(
        path.join("failures.json"),
        format!("{}\n", serde_json::to_string_pretty(&failures)?),
    )?;
    Ok(())
}

use matrix_client::Event;
use ts_rs::TS;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/desktop/src/bindings");
    Event::export_all(&ts_rs::Config::new().with_out_dir(path))?;
    Ok(())
}

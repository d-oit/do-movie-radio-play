use assert_cmd::Command;

#[test]
fn decode_smoke_on_downloaded_movie_if_present() {
    let path = "testdata/raw/bruder-1929.webm";
    if !std::path::Path::new(path).exists() {
        return;
    }
    let out = "testdata/generated/smoke.json";
    Command::cargo_bin("timeline")
        .unwrap_or_else(|_| panic!("bin"))
        .args(["extract", path, "--output", out])
        .assert()
        .success();
}

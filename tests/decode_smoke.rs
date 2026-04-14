use assert_cmd::Command;
use std::path::Path;
use std::process::Command as StdCommand;

fn ffmpeg_available() -> bool {
    StdCommand::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn downloaded_movie() -> Option<&'static str> {
    if !ffmpeg_available() {
        return None;
    }

    [
        "testdata/raw/sintel_trailer_2010.mp4",
        "testdata/raw/big_buck_bunny_trailer_2008.mov",
        "testdata/raw/elephants_dream_2006.mp4",
        "testdata/raw/eggs_1970.mp4",
        "testdata/raw/windy_day_1967.mp4",
        "testdata/raw/the_hole_1962.mp4",
        "testdata/raw/dinner_time_1928.webm",
        "testdata/raw/the_singing_fool_1928.webm",
        "testdata/raw/bruder-1929.webm",
    ]
    .into_iter()
    .find(|path| Path::new(path).exists())
}

#[test]
fn decode_smoke_on_downloaded_movie_if_present() {
    let Some(path) = downloaded_movie() else {
        return;
    };
    let out = "testdata/generated/smoke.json";
    Command::cargo_bin("timeline")
        .unwrap_or_else(|_| panic!("bin"))
        .args(["extract", path, "--output", out])
        .assert()
        .success();
}

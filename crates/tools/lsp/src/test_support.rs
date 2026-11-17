use som_driver::Analysis;

pub(crate) const POINTS: &str = "struct Point { pub x: i32, pub y: i32 }\n\
                                 fn make(x: i32) -> Point { Point { x, y: 1 } }\n\
                                 fn main() -> i32 { let p: Point = make(2); p.x + p.y }\n";

/// `source` as a program's `main.som`, analyzed; the directory lives as long as the guard.
pub(crate) fn analyzed(source: &str) -> (tempfile::TempDir, String, Analysis) {
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("main.som");
    std::fs::write(&main, source).unwrap();
    let a = som_driver::analyze(&main, &Default::default());
    (dir, main.to_string_lossy().into_owned(), a)
}

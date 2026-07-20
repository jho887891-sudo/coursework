use std::{env, process};
fn main() {
    let Some(path) = env::args().nth(1) else {
        eprintln!("usage: final-project-starter <bid.txt>");
        process::exit(2)
    };
    eprintln!("TODO: load {path}, call review(), save report and trace");
    process::exit(3);
}

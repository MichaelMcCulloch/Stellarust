use std::{fs::File, os::unix::prelude::MetadataExt, time::Instant};

use clausewitz_parser::clausewitz::root::root;
use memmap::Mmap;

fn main() {
    let filename = "/home/michael/Dev/stellarust/res/test_data/campaign_raw/unitednationsofearth_-15512622/autosave_2200.02.01/gamestate_large";
    let file = File::open(filename).expect("File not found");
    let mmap = unsafe { Mmap::map(&file).expect(&format!("Error mapping file {:?}", file)) };

    let str = std::str::from_utf8(&mmap[..]).unwrap();

    let start = Instant::now();

    let _ = root(str);

    let end = start.elapsed();

    let size_in_bytes = file.metadata().unwrap().size();
    let speed = (size_in_bytes as u128 / end.as_millis()) * 1000;

    println!(
        "{:?}MB/s, took {} seconds.",
        speed as f32 / 1000000 as f32,
        end.as_secs()
    );
}

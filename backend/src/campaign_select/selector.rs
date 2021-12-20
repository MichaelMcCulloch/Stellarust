use crate::campaign_select::retreiver;
use std::{
    fs,
    io::{stdout, Write},
    path::PathBuf,
};
use stellarust::dto::CampaignDto;
use text_io::read;

#[cfg(target_os = "linux")]
const SAVE_DATA_PATH: &str = ".local/share/Paradox Interactive/Stellaris/save games/";

pub struct CampaignSelector {}

impl CampaignSelector {
    pub fn select() -> PathBuf {
        let home = std::env::var("HOME").unwrap();
        let home_str = home.as_str();
        let path = PathBuf::from_iter(vec![home_str, SAVE_DATA_PATH]);
        Self::select_from_path(&PathBuf::from(path))
    }

    pub fn select_from_path(dir: &PathBuf) -> PathBuf {
        println!("Reading list of saves...");
        let read_dir = fs::read_dir(dir).unwrap();
        let paths: Vec<PathBuf> = read_dir
            .into_iter()
            .filter_map(|r| {
                if let Ok(dir_entry) = r {
                    Some(dir_entry.path())
                } else {
                    None
                }
            })
            .collect();
        let campaign_options = retreiver::get_campaign_options(paths);
        let keys: Vec<(usize, CampaignDto)> =
            campaign_options.clone().into_keys().enumerate().collect();
        println!("Please Select Your Save by Number:");
        for key in keys.clone() {
            println!("{}.\t{}", key.0, key.1)
        }
        let _ = stdout().flush();

        let index: usize = read!();
        let selection = keys.get(index).unwrap();
        let selected_path = campaign_options.get(&selection.1);

        selected_path.unwrap().clone()
    }
}

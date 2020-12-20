use asus_nb::aura_modes::AuraModes;
use log::{error, warn};
use rog_fan_curve::Curve;
use serde_derive::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};

pub static CONFIG_PATH: &str = "/etc/asusd/asusd.conf";

#[derive(Default, Deserialize, Serialize)]
pub struct Config {
    pub gfx_managed: bool,
    pub active_profile: String,
    pub toggle_profiles: Vec<String>,
    // TODO: remove power_profile
    pub power_profile: u8,
    pub bat_charge_limit: u8,
    pub kbd_led_brightness: u8,
    pub kbd_backlight_mode: u8,
    pub kbd_backlight_modes: Vec<AuraModes>,
    pub power_profiles: BTreeMap<String, Profile>,
}

impl Config {
    /// `load` will attempt to read the config, and panic if the dir is missing
    pub fn load(mut self, supported_led_modes: &[u8]) -> Self {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&CONFIG_PATH)
            .expect(&format!(
                "The file {} or directory /etc/asusd/ is missing",
                CONFIG_PATH
            )); // okay to cause panic here
        let mut buf = String::new();
        if let Ok(l) = file.read_to_string(&mut buf) {
            if l == 0 {
                self = Config::create_default(&mut file, &supported_led_modes);
            } else {
                self = serde_json::from_str(&buf).unwrap_or_else(|_| {
                    warn!("Could not deserialise {}", CONFIG_PATH);
                    panic!("Please remove {} then restart asusd", CONFIG_PATH);
                });
            }
        }
        self
    }

    fn create_default(file: &mut File, supported_led_modes: &[u8]) -> Self {
        // create a default config here
        let mut config = Config::default();
        config.gfx_managed = true;

        config.bat_charge_limit = 100;
        config.kbd_backlight_mode = 0;
        config.kbd_led_brightness = 1;

        for n in supported_led_modes {
            config.kbd_backlight_modes.push(AuraModes::from(*n))
        }

        let mut profile = Profile::default();
        profile.fan_preset = 0;
        profile.turbo = true;
        config.power_profiles.insert("normal".into(), profile);

        let mut profile = Profile::default();
        profile.fan_preset = 1;
        profile.turbo = true;
        config.power_profiles.insert("boost".into(), profile);

        let mut profile = Profile::default();
        profile.fan_preset = 2;
        config.power_profiles.insert("silent".into(), profile);

        config.toggle_profiles.push("normal".into());
        config.toggle_profiles.push("boost".into());
        config.toggle_profiles.push("silent".into());
        config.active_profile = "normal".into();

        // Should be okay to unwrap this as is since it is a Default
        let json = serde_json::to_string_pretty(&config).unwrap();
        file.write_all(json.as_bytes())
            .unwrap_or_else(|_| panic!("Could not write {}", CONFIG_PATH));
        config
    }

    pub fn read(&mut self) {
        let mut file = OpenOptions::new()
            .read(true)
            .open(&CONFIG_PATH)
            .unwrap_or_else(|err| panic!("Error reading {}: {}", CONFIG_PATH, err));
        let mut buf = String::new();
        if let Ok(l) = file.read_to_string(&mut buf) {
            if l == 0 {
                warn!("File is empty {}", CONFIG_PATH);
            } else {
                let x: Config = serde_json::from_str(&buf)
                    .unwrap_or_else(|_| panic!("Could not deserialise {}", CONFIG_PATH));
                *self = x;
            }
        }
    }

    pub fn read_new() -> Result<Config, Box<dyn std::error::Error>> {
        let mut file = OpenOptions::new()
            .read(true)
            .open(&CONFIG_PATH)
            .unwrap_or_else(|err| panic!("Error reading {}: {}", CONFIG_PATH, err));
        let mut buf = String::new();
        file.read_to_string(&mut buf)?;
        let x: Config = serde_json::from_str(&buf)?;
        Ok(x)
    }

    pub fn write(&self) {
        let mut file = File::create(CONFIG_PATH).expect("Couldn't overwrite config");
        let json = serde_json::to_string_pretty(self).expect("Parse config to JSON failed");
        file.write_all(json.as_bytes())
            .unwrap_or_else(|err| error!("Could not write config: {}", err));
    }

    pub fn set_mode_data(&mut self, mode: AuraModes) {
        let byte: u8 = (&mode).into();
        for (index, n) in self.kbd_backlight_modes.iter().enumerate() {
            if byte == u8::from(n) {
                // Consume it, OMNOMNOMNOM
                self.kbd_backlight_modes[index] = mode;
                break;
            }
        }
    }

    pub fn get_led_mode_data(&self, num: u8) -> Option<&AuraModes> {
        for mode in &self.kbd_backlight_modes {
            if u8::from(mode) == num {
                return Some(mode);
            }
        }
        None
    }
}

#[derive(Deserialize, Serialize)]
pub struct Profile {
    pub min_percentage: u8,
    pub max_percentage: u8,
    pub turbo: bool,
    pub fan_preset: u8,
    pub fan_curve: Option<Curve>,
}

#[deprecated]
pub type CPUSettings = Profile;

impl Default for Profile {
    fn default() -> Self {
        Profile {
            min_percentage: 0,
            max_percentage: 100,
            turbo: false,
            fan_preset: 0,
            fan_curve: None,
        }
    }
}
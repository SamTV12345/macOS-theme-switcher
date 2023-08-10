#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use std::cell::OnceCell;
use std::fs::File;
use std::io::{Read, Write};
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};
use std::{env, thread};
use std::thread::sleep;
use std::time::{Duration, SystemTime};
use chrono::{DateTime, Timelike, Utc};
use job_scheduler_ng::{Job, JobScheduler};
use reqwest::Error;
use serde::{Deserialize, Serialize};
use tauri::{ActivationPolicy, CustomMenuItem, Manager, SystemTray, SystemTrayEvent, SystemTrayMenu};
use tauri::State;
use tauri_plugin_positioner::{Position, WindowExt};
use tokio::runtime::Runtime;

#[derive(Serialize, Deserialize)]
enum Theme {
    Light,
    Dark
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    automatic_switching: bool
}

impl Config {
    fn new() -> Config {
        return Config{
            automatic_switching: true
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct SunRiseData {
    results: SunriseDataResult
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct SunriseDataResult {
    sunrise: DateTime<Utc>,
    sunset: DateTime<Utc>,
    solar_noon: DateTime<Utc>,
    day_length: i32,
    civil_twilight_begin: DateTime<Utc>,
    civil_twilight_end: DateTime<Utc>,
    nautical_twilight_begin: DateTime<Utc>,
    nautical_twilight_end: DateTime<Utc>,
    astronomical_twilight_begin: DateTime<Utc>,
    astronomical_twilight_end: DateTime<Utc>
}

fn get_sunset_data() ->Result<SunRiseData, Error>{
    println!("Getting data");
    reqwest::blocking::get("https://api.sunrise-sunset.org/json?formatted=0")
        .unwrap()
        .json::<SunRiseData>()
}

#[tauri::command]
async fn get_config() -> Config{
    let mut file = File::open(SETTINGS_PATH.get().unwrap()).unwrap();
    get_config_(&mut file)
}

#[tauri::command]
async fn change_sunset_option(activated:bool, sun_data: State<'_,Arc<Mutex<Option<SunRiseData>>>>) ->Result<(), ()>{
    match activated {
        true => {
            let mut data = sun_data.clone();
            if data.inner().lock().unwrap().is_none(){
                println!("Retrieving data from sunset data.");
                let new_data = tokio::task::spawn_blocking(||get_sunset_data().unwrap()).await.unwrap();
                *data.inner().lock().unwrap() = Option::from(new_data);
            }
            let sun_data = data.inner().lock().unwrap().clone();
            calc_theme_from_sundata(sun_data.unwrap()).await;
        }
        false => {
            // Nothing to do. Just leave it as is.
        }
    }
    let config = Config{
        automatic_switching: activated
    };
    Ok(write_config_to_file(config))
}


async fn calc_theme_from_sundata(sun_data: SunRiseData){
    let sunrise = sun_data.results.sunrise;
    let sunset = sun_data.results.sunset;
    let system_time:DateTime<Utc> = SystemTime::now().into();

    // 19 sunset, current 19:30, sunrise 6:00
    if system_time.hour().ge(&sunset.hour()) && system_time.hour().le(&sunrise.hour()){
        println!("Change to dark");
        change_theme(Theme::Dark).await
    }
    else{
        println!("Change to light");
        change_theme(Theme::Light).await
    }

}

#[tauri::command]
async fn change_theme_handler(theme_selection: Theme){
    change_theme(theme_selection).await
}

async fn change_theme(theme_selection: Theme){
    let app_theme_selection;
    match theme_selection {
        Theme::Light => {
            app_theme_selection = "Application('System Events').appearancePreferences.darkMode = false";
        }
        Theme::Dark => {
            app_theme_selection = "Application('System Events').appearancePreferences.darkMode = true";
        }
    }
    Command::new("osascript")
        .args(&["-l", "JavaScript","-e",app_theme_selection])
        .spawn()
        .expect("Error changing theme");
}


fn prepare_config() -> Config{
    return match File::open(SETTINGS_PATH.get().unwrap()) {
        Ok(mut f) => {
            get_config_(&mut f)
        }
        Err(e) => {
            let mut created_file = File::create(SETTINGS_PATH.get().unwrap()).expect("");

            let default_option = Config::new();


            created_file.write(serde_json::to_string(&default_option).unwrap().as_bytes())
                .expect("Error writing file");

            default_option
        }
    }
}

fn get_config_(f: &mut File) -> Config {
    let mut buffer = String::new();
    f.read_to_string(&mut buffer).unwrap();

    let config: Config = serde_json::from_str(&buffer).unwrap();

    config
}


fn write_config_to_file(config:Config) {
    let mut created_file = File::create(SETTINGS_PATH.get().unwrap()).expect("");

    created_file.write_all(serde_json::to_string(&config).unwrap().as_bytes())
        .expect("Error writing file");
}


static SETTINGS_PATH:OnceLock<String> = OnceLock::new();

fn main() {
    let mut dir = env::var("APP_DIR").unwrap_or("..".to_string());
    dir.push_str("/settings.json");
    SETTINGS_PATH.set(dir).expect("TODO: panic message");

    let quit = CustomMenuItem::new("quit".to_string(), "Quit").accelerator("Cmd+Q");
    let system_tray_menu = SystemTrayMenu::new().add_item(quit);



    let config = prepare_config();

    let mutexed_config = Arc::new( Mutex::new(config));
    let mutexed_opt_sunset_data:Arc<Mutex<Option<SunRiseData>>> = Arc::new(Mutex::new(Option::default()));

    let cloned_config = Arc::clone(&mutexed_config);
    let cloned_sunset_data = Arc::clone(&mutexed_opt_sunset_data);

    thread::spawn(||{
        execute_schedulers(cloned_config, cloned_sunset_data);
    });


    let mut app = tauri::Builder::default()
        .manage(Arc::clone(&mutexed_config))
        .manage(Arc::clone(&mutexed_opt_sunset_data))
        .invoke_handler(tauri::generate_handler![change_theme_handler, change_sunset_option, get_config])
        .plugin(tauri_plugin_positioner::init())
        .system_tray(SystemTray::new().with_menu(system_tray_menu))
        .on_system_tray_event(|app, event| {
            tauri_plugin_positioner::on_tray_event(app, &event);
            match event {
                SystemTrayEvent::LeftClick {
                    position: _,
                    size: _,
                    ..
                } => {
                    let window = app.get_window("main").unwrap();
                    let _ = window.move_window(Position::TrayCenter);

                    if window.is_visible().unwrap() {
                        window.hide().unwrap();
                    } else {
                        window.show().unwrap();
                        window.set_focus().unwrap();
                    }
                }
                SystemTrayEvent::RightClick {
                    position: _,
                    size: _,
                    ..
                } => {
                    println!("system tray received a right click");
                }
                SystemTrayEvent::DoubleClick {
                    position: _,
                    size: _,
                    ..
                } => {
                    println!("system tray received a double click");
                }
                SystemTrayEvent::MenuItemClick { id, .. } => match id.as_str() {
                    "quit" => {
                        std::process::exit(0);
                    }
                    "hide" => {
                        let window = app.get_window("main").unwrap();
                        window.hide().unwrap();
                    }
                    _ => {}
                },
                _ => {}
            }
        })
        .on_window_event(|event| match event.event() {
            tauri::WindowEvent::Focused(is_focused) => {
                // detect click outside of the focused window and hide the app
                if !is_focused {
                    event.window().hide().unwrap();
                }
            }
            _ => {}
        }).build(tauri::generate_context!())
        .expect("Error starting the app. It is only supported on mac os.");
    app.set_activation_policy(ActivationPolicy::Accessory);

    app.run(|_app_handle, _event| {});
}


fn execute_schedulers(config: Arc<Mutex<Config>>, sun_data: Arc<Mutex<Option<SunRiseData>>>){
    let mut scheduler = JobScheduler::new();

        let mutexed_config = Arc::clone(&config);
        let mutexed_sunset_data = Arc::clone(&sun_data);
        scheduler.add(Job::new("0 5 * * * * *".parse().unwrap(),move || {
            let current_config = mutexed_config.lock().unwrap();
            println!("Current config");
            if current_config.automatic_switching {
                let rt = Runtime::new().unwrap();
                rt.block_on(calc_theme_from_sundata(mutexed_sunset_data.lock().unwrap().clone().unwrap()));
            }
        }));


        let mutexed_config_2 = Arc::clone(&config);
        let mutexed_sunset_data = Arc::clone(&sun_data);

        scheduler.add(Job::new("0 0 6 * * * *".parse().unwrap(),move || {
            let config = mutexed_config_2.lock().unwrap();
            println!("Getting data");
            if config.automatic_switching {
                let mut sunset_data = mutexed_sunset_data.lock().unwrap();
                let data = get_sunset_data();
                println!("{:?}", data);
                *sunset_data = Option::from(data.unwrap());
            }
        }));

    loop {
        scheduler.tick();
        sleep(Duration::from_millis(1000));
    }
}

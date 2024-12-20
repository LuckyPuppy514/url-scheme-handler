#![windows_subsystem = "windows"]

use base64::{engine::general_purpose, Engine};
use eframe::egui;
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use std::{
    env,
    error::Error,
    ffi::OsStr,
    fs::{self, File},
    io::{Read, Write},
    os::windows::{ffi::OsStrExt, process::CommandExt},
    path::Path,
    process::Command,
    ptr::null_mut,
};
use urlencoding::decode;
use winapi::um::winuser::{MessageBoxW, MB_ICONINFORMATION, MB_OK};
use winreg::{enums::*, RegKey};

#[derive(Serialize, Deserialize, Debug, Default)]
struct Config {
    is_registry_added: bool,
    apps: Vec<AppConfig>,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
struct AppConfig {
    name: String,
    path: Option<String>,
}

fn decompress(compressed_str: &str) -> Result<String, std::string::FromUtf8Error> {
    let compressed_bytes = general_purpose::STANDARD
        .decode(compressed_str)
        .expect("Failed to decode base64 string");
    let mut decoder = GzDecoder::new(&compressed_bytes[..]);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed).unwrap();
    String::from_utf8(decompressed)
}

fn add_url_scheme() -> Result<(), Box<dyn Error>> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _disp) = hkcu.create_subkey("Software\\Classes\\ush")?;
    key.set_value("", &"URL: USH Protocol")?;
    let command = format!(
        "\"{}\\url-scheme-handler.exe\" run \"%1\"",
        std::env::current_dir()?.display()
    );
    key.set_value("URL Protocol", &"")?;
    let (command_key, _disp) = key.create_subkey("shell\\open\\command")?;
    command_key.set_value("", &command)?;

    Ok(())
}

fn remove_url_scheme() -> Result<(), Box<dyn Error>> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.delete_subkey_all("Software\\Classes\\ush")?;

    Ok(())
}

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

fn show_message_box(title: &str, message: &str) {
    unsafe {
        MessageBoxW(
            null_mut(),
            to_wide(message).as_ptr(),
            to_wide(title).as_ptr(),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

fn open_setting_widow() {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(egui::vec2(700.0, 350.0))
            .with_resizable(true),
        centered: true,
        ..Default::default()
    };
    let _ = eframe::run_native(
        "URL Scheme Handler",
        options,
        Box::new(|_cc| Ok(Box::new(UrlSchemeHandler::default()))),
    );
}

impl Config {
    fn load_from_file(path: &str) -> Self {
        if Path::new(path).exists() {
            let config_data = fs::read_to_string(path).expect("Failed to read config file");
            serde_json::from_str(&config_data).unwrap_or_default()
        } else {
            Config::default()
        }
    }

    fn save_to_file(&self, path: &str) {
        let config_json = serde_json::to_string_pretty(self).expect("Failed to serialize config");
        let mut file = File::create(path).expect("Failed to create config file");
        file.write_all(config_json.as_bytes())
            .expect("Failed to write config file");
    }
}

fn main() -> std::io::Result<()> {
    let current_exe = env::current_exe()?;
    let app_dir = current_exe
        .parent()
        .expect("Failed to get parent directory");
    env::set_current_dir(&app_dir)?;

    let args: Vec<String> = env::args().collect();

    match args.as_slice() {
        [_] => {
            open_setting_widow();
        }
        [_, command, input] if command == "run" => {
            if let Some(stripped) = input.strip_prefix("ush://") {
                let parts: Vec<&str> = stripped.split('?').collect();

                if parts.len() == 2 {
                    let decoded =
                        decode(parts[0].trim_end_matches('/')).expect("Failed to decode URL");
                    let app_name = decoded.as_ref();
                    let gzip_args = parts[1].trim_end_matches('/');

                    if let Ok(args) = decompress(&gzip_args) {
                        let config = Config::load_from_file("config.json");
                        let app_path: &str;

                        if let Some(app) = config
                            .apps
                            .iter()
                            .find(|app| app.name.trim() == app_name.trim())
                        {
                            app_path = app.path.as_deref().unwrap_or("");
                            println!("Executing command: {} {}", app_path, args);
                            let output = Command::new(app_path).raw_arg(args).output();

                            match output {
                                Ok(output) => {
                                    if !output.status.success() {
                                        let stderr = String::from_utf8_lossy(&output.stderr);
                                        let stdout = String::from_utf8_lossy(&output.stdout);
                                        show_message_box(
                                            "Error",
                                            &format!("{}\n{}", stderr, stdout),
                                        );
                                    } else {
                                        println!("Command executed successfully");
                                    }
                                }
                                Err(e) => {
                                    show_message_box(
                                        "Error",
                                        &format!("{}\n{}", app_path, &e.to_string()),
                                    );
                                    open_setting_widow();
                                }
                            }
                        } else {
                            show_message_box(
                                "Error",
                                &format!("No app found with name: {}", app_name),
                            );
                            open_setting_widow();
                        }
                    } else {
                        show_message_box("Error", "Failed to decompress gzip args");
                    }
                } else {
                    show_message_box("Error", "Input format is incorrect");
                }
            } else {
                show_message_box("Error", "Input does not start with 'ush://'");
            }
        }
        _ => {
            show_message_box(
                "Error",
                "Invalid arguments. Usage: run ush://${app_name}?${gzip_args}",
            );
        }
    }
    Ok(())
}

struct UrlSchemeHandler {
    config: Config,
}

impl Default for UrlSchemeHandler {
    fn default() -> Self {
        let config = Config::load_from_file("config.json");
        Self { config }
    }
}

impl UrlSchemeHandler {}

impl eframe::App for UrlSchemeHandler {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(10.0);

                let label_width = 100.0;
                let label_height = 30.0;

                egui::Frame::none().show(ui, |ui| {
                    ui.set_min_height(200.0);
                    ui.set_max_height(200.0);
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let mut remove_indices = Vec::new();
                        let mut apps_clone = self.config.apps.clone();

                        for (index, app) in apps_clone.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                ui.set_min_height(label_height);

                                let app_name_input = ui.add_sized(
                                    [label_width, label_height],
                                    egui::TextEdit::singleline(&mut self.config.apps[index].name)
                                        .vertical_align(egui::Align::Center)
                                        .horizontal_align(egui::Align::Center),
                                );

                                if app_name_input.lost_focus() {
                                    self.config.save_to_file("config.json");
                                }

                                if ui
                                    .add_sized(
                                        [ui.available_width() - 50.0, ui.available_height()],
                                        egui::Button::new(
                                            app.path.as_deref().unwrap_or("Select Player Path"),
                                        ),
                                    )
                                    .clicked()
                                {
                                    if let Some(path) = rfd::FileDialog::new()
                                        .add_filter("Executable", &["exe"])
                                        .pick_file()
                                    {
                                        if path.display().to_string().is_empty() {
                                            println!("The path is empty.");
                                        } else {
                                            self.config.apps[index].path =
                                                Some(path.display().to_string());
                                            self.config.save_to_file("config.json");
                                        }
                                    }
                                }
                                if ui
                                    .add_sized(
                                        [ui.available_width(), ui.available_height()],
                                        egui::Button::new("➖"),
                                    )
                                    .clicked()
                                {
                                    remove_indices.push(index);
                                }
                            });
                            ui.add_space(10.0);
                        }

                        for index in remove_indices.into_iter().rev() {
                            self.config.apps.remove(index);
                            self.config.save_to_file("config.json");
                        }
                    });
                });

                ui.add_space(20.0);
                ui.separator();
                ui.add_space(10.0);

                if ui
                    .add_sized(
                        [ui.available_width(), label_height],
                        egui::Button::new("➕"),
                    )
                    .clicked()
                {
                    self.config.apps.push(AppConfig::default());
                }

                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    let button_width = ui.available_width() * 0.50 - 7.0;

                    if ui
                        .add_sized(
                            [button_width, 30.0],
                            egui::Button::new(if self.config.is_registry_added {
                                format!("✅ {}", "Add to Registry")
                            } else {
                                format!("➕ {}", "Add to Registry")
                            }),
                        )
                        .clicked()
                    {
                        if let Err(e) = add_url_scheme() {
                            show_message_box("Error", &format!("Adding to registry failed: {}", e));
                        } else {
                            self.config.is_registry_added = true;
                            self.config.save_to_file("config.json");
                            show_message_box("Info", "Adding to registry success");
                        }
                    }

                    ui.add_space(5.0);

                    if ui
                        .add_sized(
                            [button_width, 30.0],
                            egui::Button::new("➖ Remove from Registry"),
                        )
                        .clicked()
                    {
                        if let Err(e) = remove_url_scheme() {
                            show_message_box(
                                "Error",
                                &format!("Removing from registry failed: {}", e),
                            );
                        } else {
                            self.config.is_registry_added = false;
                            self.config.save_to_file("config.json");
                            show_message_box("Info", "Removing from registry success");
                        }
                    }
                });
                ui.add_space(10.0);
            });
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.config.save_to_file("config.json");
    }
}

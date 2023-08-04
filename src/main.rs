use eframe::NativeOptions;
use egui::{CentralPanel, Sense, Slider, TextEdit, Window};
use egui_video::{AudioDevice, Player};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};

fn main() {
    let _ = eframe::run_native(
        "Simple video tags",
        NativeOptions::default(),
        Box::new(|_| Box::new(App::default())),
    );
}

fn file_hash(path: String) -> String {
    let bytes = std::fs::read(path).unwrap(); // Vec<u8>
    sha256::digest(&bytes)
}

#[derive(Debug, Serialize, Deserialize)]
struct Tags {
    options: HashSet<String>,
    /// indexed by video hash string
    db: HashMap<String, HashSet<String>>,
}

impl Tags {
    fn new() -> Self {
        Self {
            options: HashSet::new(),
            db: HashMap::new(),
        }
    }
}

/// Heavily based on egui-video example...
struct App {
    audio_device: AudioDevice,
    player: Option<Player>,
    folder: String,
    videos: Vec<String>,
    tags: Tags,
    media_idx: Option<usize>,
    media_path: String,
    stream_size_scale: f32,
    seek_frac: f32,
}

impl Default for App {
    fn default() -> Self {
        // load saved data
        let mut tags = Tags::new();
        if Path::new("tags.json").exists() {
            tags = serde_json::from_str(
                fs::read_to_string("tags.json")
                    .expect("tags.json load err")
                    .as_str(),
            )
            .expect("tags.json deserialize err");
            dbg!(&tags);
        } else {
            fs::write("tags.json", serde_json::to_string(&tags).unwrap());
        }
        Self {
            audio_device: egui_video::init_audio_device(&sdl2::init().unwrap().audio().unwrap())
                .unwrap(),
            folder: String::new(),
            videos: vec![],
            tags,
            media_idx: None,
            media_path: String::new(),
            stream_size_scale: 1.,
            seek_frac: 0.,
            player: None,
        }
    }
}
impl App {
    fn new_player(&mut self, ctx: &egui::Context) {
        self.media_path = self
            .media_idx
            .map_or(String::new(), |i| self.videos[i].clone());
        match Player::new(ctx, &self.media_path.replace("\"", ""))
            .and_then(|p| p.with_audio(&mut self.audio_device))
        {
            Ok(player) => {
                self.player = Some(player);
            }
            Err(e) => println!("failed to make stream: {e}"),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint();
        CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                let tedit_resp = ui.add_sized(
                    [ui.available_width(), ui.available_height()],
                    TextEdit::singleline(&mut self.folder)
                        .hint_text("click to set path")
                        .interactive(false),
                );

                if ui
                    .interact(
                        tedit_resp.rect,
                        tedit_resp.id.with("click_sense"),
                        Sense::click(),
                    )
                    .clicked()
                {
                    if let Some(path_buf) = rfd::FileDialog::new().pick_folder() {
                        self.folder = path_buf.as_path().to_string_lossy().to_string();
                        self.videos = fs::read_dir(&self.folder)
                            .expect("could not read folder?")
                            .filter_map(|entry| entry.ok())
                            .map(|entry| {
                                String::from(entry.path().to_str().expect("valid unicode"))
                            })
                            .filter(|name| {
                                ["mp4", "gif", "webm"]
                                    .iter()
                                    .find(|ext| name.contains(format!(".{ext}").as_str()))
                                    .is_some()
                            })
                            .collect();
                        self.media_idx = if self.videos.len() > 0 { Some(0) } else { None };
                        self.new_player(ctx);
                    }
                }
            });
            ui.label(format!("{} video files in folder", self.videos.len()));
            if let Some(i) = self.media_idx {
                ui.label(format!("selected: {:?} ({})", &self.media_path, i + 1));
            }
            ui.separator();
            if let Some(player) = self.player.as_mut() {
                player.ui(
                    ui,
                    [
                        player.width as f32 * self.stream_size_scale,
                        player.height as f32 * self.stream_size_scale,
                    ],
                );
            }
            Window::new("video controls").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let n = self.videos.len();
                    if n > 0 {
                        if ui.button("Prev").clicked() {
                            self.media_idx = self.media_idx.map(|i| (i + n - 1) % n);
                            self.new_player(ctx);
                        }
                        if ui.button("Next").clicked() {
                            self.media_idx = self.media_idx.map(|i| (i + 1) % n);
                            self.new_player(ctx);
                        }
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label("size scale");
                            ui.add(Slider::new(&mut self.stream_size_scale, 0.0..=2.));
                        });
                        if let Some(player) = self.player.as_mut() {
                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.label("volume");
                                let mut volume = player.audio_volume.get();
                                if ui
                                    .add(Slider::new(&mut volume, 0.0..=player.max_audio_volume))
                                    .changed()
                                {
                                    player.audio_volume.set(volume);
                                };
                            });
                        }
                        if !self.media_path.is_empty() {
                            let fh = file_hash(self.media_path.clone());
                            if !self.tags.db.contains_key(&fh) {
                                self.tags.db.insert(fh.clone(), HashSet::new());
                            }
                            ui.separator();
                            ui.label("Tags");
                            for opt in self.tags.options.iter() {
                                let mut temp = self.tags.db[&fh].contains(opt);
                                ui.checkbox(&mut temp, opt.clone());
                                let vid_tags = self.tags.db.get_mut(&fh).unwrap();
                                match temp {
                                    true => vid_tags.insert(opt.clone()),
                                    false => vid_tags.remove(opt),
                                };
                            }
                            if ui.button("Save tags").clicked() {
                                fs::write("tags.json", serde_json::to_string(&self.tags).unwrap());
                            }
                        }
                    }
                })
            });
        });
    }
}

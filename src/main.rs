use eframe::NativeOptions;
use egui::{CentralPanel, Sense, Slider, TextEdit, Window};
use egui_video::{AudioDevice, Player};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs::{self},
    path::{Path, PathBuf},
};

fn main() {
    let _ = eframe::run_native(
        "Simple video tags",
        NativeOptions::default(),
        Box::new(|_| Box::new(App::default())),
    );
}

fn file_hash<P>(path: P) -> String
where
    P: AsRef<Path>,
{
    let bytes = std::fs::read(path).unwrap(); // Vec<u8>
    sha256::digest(&bytes)
}

fn has_allowed_extension(path: &PathBuf) -> bool {
    ["mp4", "gif", "webm"]
        .iter()
        .find(|allowed_ext| {
            if let Some(file_ext) = path.extension() {
                **allowed_ext == file_ext.to_str().unwrap()
            } else {
                false
            }
        })
        .is_some()
}

fn write_json<S>(name: &str, serializable: &S)
where
    S: Serialize,
{
    let filename = format!("{name}.json");
    fs::write(
        filename.clone(),
        serde_json::to_string(serializable).expect("{name} to_string fail"),
    )
    .expect("failed to write {filename}");
    println!("wrote {filename}");
}

fn load_json<D>(name: &str) -> D
where
    D: DeserializeOwned,
{
    let filename = format!("{name}.json");
    let file_contents = fs::read_to_string(filename).expect("{filename} load err");
    serde_json::from_str(file_contents.as_str()).expect("{filename} deserialize err")
}

/// expects to be handed a list of files from the same folder
fn folder_hashes(paths: &Vec<PathBuf>, update: bool) -> HashMap<String, PathBuf> {
    let parent = paths[0].parent().unwrap();
    let hash_path = parent.join(".hashes");
    let hash_filename = hash_path.to_str().unwrap();
    let fbh: FilesByHash = if parent.join(".hashes.json").exists() && !update {
        load_json(hash_filename)
    } else {
        let temp = FilesByHash {
            db: paths
                .iter()
                .map(|vid| {
                    (
                        file_hash(vid),
                        String::from(vid.file_name().unwrap().to_str().unwrap()),
                    )
                })
                .collect(),
        };
        write_json(hash_filename, &temp);
        temp
    };
    fbh.db
        .iter()
        .map(|(hash, filename)| (hash.clone(), parent.join(filename)))
        .collect()
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

/// Stores a local file reference (as absolute paths can still be relative to mountpoint...)
#[derive(Debug, Serialize, Deserialize)]
struct FilesByHash {
    db: HashMap<String, String>,
}

/// Heavily based on egui-video example...
struct App {
    audio_device: AudioDevice,
    player: Option<Player>,
    videos: Vec<PathBuf>,
    paths_from_hash: HashMap<String, PathBuf>,
    videos_filtered: Vec<PathBuf>,
    update_hashes_on_load: bool,
    tags: Tags,
    tag_filter: HashSet<String>,
    media_idx: Option<usize>,
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
            // dbg!(&tags);
        } else {
            write_json("tags", &tags);
        }
        Self {
            audio_device: egui_video::init_audio_device(&sdl2::init().unwrap().audio().unwrap())
                .unwrap(),
            videos: vec![],
            paths_from_hash: HashMap::new(),
            videos_filtered: vec![],
            update_hashes_on_load: false,
            tags,
            tag_filter: HashSet::new(),
            media_idx: None,
            player: None,
        }
    }
}

impl App {
    fn new_player(&mut self, ctx: &egui::Context) {
        let media_path = self.media_idx.map_or(String::new(), |i| {
            String::from(self.videos_filtered[i].to_str().unwrap())
        }); // empty if idx is None
        self.player = Player::new(ctx, &media_path.replace("\"", ""))
            .and_then(|p| p.with_audio(&mut self.audio_device))
            .ok()
    }

    fn load_folder(&mut self, path_buf: PathBuf) {
        let mut vids: Vec<_> = fs::read_dir(path_buf)
            .expect("could not read folder?")
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(has_allowed_extension)
            .collect();
        for (hash, pb) in folder_hashes(&vids, self.update_hashes_on_load) {
            self.paths_from_hash.insert(hash, pb);
        }
        self.videos.append(&mut vids);
    }

    fn update_filtered(&mut self) {
        self.videos_filtered = Vec::from_iter(
            self.paths_from_hash
                .iter()
                .filter(|(hash, _)| {
                    self.tag_filter
                        .iter()
                        .find(|tag| {
                            if !self.tags.db.contains_key(*hash) || self.tags.db.len() == 0 {
                                true
                            } else {
                                !self.tags.db[*hash].contains(*tag)
                            }
                        })
                        .is_none()
                })
                .map(|(_, pb)| pb.clone()),
        );
        if self.videos_filtered.len() > 0 {
            if let None = self.media_idx {
                self.media_idx = if self.videos_filtered.len() > 0 {
                    Some(0)
                } else {
                    None
                };
            }
        } else {
            self.media_idx = None;
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint();
        CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.update_hashes_on_load, "recalculate folder hashes");

                let mut temp = String::new();
                let tedit_resp = ui.add_sized(
                    [ui.available_width(), ui.available_height()],
                    TextEdit::singleline(&mut temp)
                        .hint_text("click to add folder")
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
                        self.load_folder(path_buf);
                        self.new_player(ctx);
                    }
                }
            });
            ui.label(format!("{} video files loaded", self.videos.len()));
            ui.horizontal(|ui| {
                ui.label("Filter for those containing");
                let mut any_changed = false;
                for opt in self.tags.options.iter() {
                    let mut temp = self.tag_filter.contains(opt);
                    if ui.checkbox(&mut temp, opt.clone()).changed() {
                        any_changed = true;
                    }
                    match temp {
                        true => self.tag_filter.insert(opt.clone()),
                        false => self.tag_filter.remove(opt),
                    };
                }
                if any_changed {
                    self.update_filtered();
                    self.new_player(ctx);
                }
            });
            ui.label(format!(
                "{} of {} satisfy filter",
                self.videos_filtered.len(),
                self.videos.len()
            ));
            if let Some(i) = self.media_idx {
                ui.label(format!(
                    "selected: {:?} ({} of {})",
                    &self.videos_filtered[i],
                    i + 1,
                    self.videos_filtered.len()
                ));
            }
            ui.separator();
            if let Some(player) = self.player.as_mut() {
                let width = ui.available_width();
                let height_ratio = (player.height as f32) / (player.width as f32);
                player.ui(ui, [width, width * height_ratio]);
            }
            ui.horizontal(|ui| {
                let n = self.videos_filtered.len();
                if n > 0 {
                    if ui.button("Prev").clicked() {
                        self.media_idx = self.media_idx.map(|i| (i + n - 1) % n);
                        self.new_player(ctx);
                    }
                    if ui.button("Next").clicked() {
                        self.media_idx = self.media_idx.map(|i| (i + 1) % n);
                        self.new_player(ctx);
                    }
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
                    if let Some(i) = self.media_idx {
                        let fh = file_hash(&self.videos_filtered[i]);
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
                            write_json("tags", &self.tags);
                        }
                    }
                }
            });
        });
    }
}

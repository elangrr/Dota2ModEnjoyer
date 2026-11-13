use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};

use dme_core::pictures;
use dme_core::vpk::Archive;
use eframe::egui;

const LARGEST_SIDE: u32 = 256;
const LOADERS: usize = 3;
const MOST_WAITING: usize = 240;
const MOST_KEPT: usize = 500;

pub struct Pictures {
    ctx: egui::Context,
    known: HashMap<String, Slot>,
    wanted: Vec<String>,
    shared: Arc<Shared>,
    read: Receiver<(String, Option<egui::ColorImage>)>,
    frame: u64,
}

enum Slot {
    Loading,
    Ready { texture: egui::TextureHandle, shown: u64 },
    Missing,
}

struct Shared {
    archive: Mutex<Option<Arc<Archive>>>,
    waiting: Mutex<Vec<String>>,
    wake: Condvar,
    closed: AtomicBool,
}

impl Pictures {
    pub fn new(ctx: &egui::Context) -> Pictures {
        let shared = Arc::new(Shared {
            archive: Mutex::new(None),
            waiting: Mutex::new(Vec::new()),
            wake: Condvar::new(),
            closed: AtomicBool::new(false),
        });
        let (sender, read) = mpsc::channel();
        for number in 0..LOADERS {
            let (shared, sender, ctx) = (Arc::clone(&shared), sender.clone(), ctx.clone());
            std::thread::Builder::new()
                .name(format!("pictures {number}"))
                .spawn(move || load_pictures(&shared, &sender, &ctx))
                .expect("a thread can be started");
        }
        Pictures { ctx: ctx.clone(), known: HashMap::new(), wanted: Vec::new(), shared, read, frame: 0 }
    }

    pub fn use_archive(&self, archive: Arc<Archive>) {
        *self.shared.archive.lock().unwrap() = Some(archive);
    }

    pub fn begin_frame(&mut self) {
        self.frame += 1;
        while let Ok((path, image)) = self.read.try_recv() {
            let slot = match image {
                Some(image) => {
                    let texture = self.ctx.load_texture(path.as_str(), image, egui::TextureOptions::LINEAR);
                    Slot::Ready { texture, shown: self.frame }
                }
                None => Slot::Missing,
            };
            self.known.insert(path, slot);
        }
        if self.frame.is_multiple_of(120) {
            self.forget_old();
        }
    }

    pub fn end_frame(&mut self) {
        if self.wanted.is_empty() {
            return;
        }
        let mut waiting = self.shared.waiting.lock().unwrap();
        waiting.extend(self.wanted.drain(..).rev());
        if waiting.len() > MOST_WAITING {
            let stale = waiting.len() - MOST_WAITING;
            for path in waiting.drain(..stale) {
                self.known.remove(&path);
            }
        }
        drop(waiting);
        self.shared.wake.notify_all();
    }

    pub fn get(&mut self, path: &str) -> Option<(egui::TextureId, egui::Vec2)> {
        match self.known.get_mut(path) {
            Some(Slot::Ready { texture, shown }) => {
                *shown = self.frame;
                Some((texture.id(), texture.size_vec2()))
            }
            Some(Slot::Loading | Slot::Missing) => None,
            None => {
                self.known.insert(path.to_string(), Slot::Loading);
                self.wanted.push(path.to_string());
                None
            }
        }
    }

    pub fn loading(&self) -> usize {
        self.known.values().filter(|slot| matches!(slot, Slot::Loading)).count()
    }

    fn forget_old(&mut self) {
        let mut shown: Vec<(u64, &String)> = self
            .known
            .iter()
            .filter_map(|(path, slot)| match slot {
                Slot::Ready { shown, .. } => Some((*shown, path)),
                _ => None,
            })
            .collect();
        if shown.len() <= MOST_KEPT {
            return;
        }
        shown.sort_unstable();
        let too_many = shown.len() - MOST_KEPT * 3 / 4;
        let on_screen = self.frame.saturating_sub(2);
        let old: Vec<String> =
            shown.into_iter().take(too_many).filter(|(frame, _)| *frame < on_screen).map(|(_, path)| path.clone()).collect();
        for path in old {
            self.known.remove(&path);
        }
    }
}

impl Drop for Pictures {
    fn drop(&mut self) {
        self.shared.closed.store(true, Ordering::Relaxed);
        self.shared.wake.notify_all();
    }
}

fn load_pictures(shared: &Shared, read: &Sender<(String, Option<egui::ColorImage>)>, ctx: &egui::Context) {
    loop {
        let path = {
            let mut waiting = shared.waiting.lock().unwrap();
            loop {
                if shared.closed.load(Ordering::Relaxed) {
                    return;
                }
                if let Some(path) = waiting.pop() {
                    break path;
                }
                waiting = shared.wake.wait(waiting).unwrap();
            }
        };
        let archive = shared.archive.lock().unwrap().clone();
        let image = archive.and_then(|archive| std::panic::catch_unwind(|| read_picture(&archive, &path)).ok().flatten());
        if read.send((path, image)).is_err() {
            return;
        }
        ctx.request_repaint();
    }
}

fn read_picture(archive: &Archive, path: &str) -> Option<egui::ColorImage> {
    let file = archive.read(path).ok()??;
    let picture = pictures::decode(&file).ok()?.fit_within(LARGEST_SIDE, LARGEST_SIDE);
    Some(egui::ColorImage::from_rgba_unmultiplied([picture.width as usize, picture.height as usize], &picture.rgba))
}

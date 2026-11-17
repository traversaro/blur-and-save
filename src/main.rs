#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod detect;
mod photo;

use std::{
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    thread,
};

use eframe::egui::{self, Color32, ColorImage, Pos2, Rect, Sense, TextureHandle, Vec2};
use image::RgbaImage;

use crate::{
    detect::Detector,
    photo::{Effect, Error, Region},
};

const NAME: &str = "Blur & Save";
const ICON: &[u8] = include_bytes!("../assets/blur-and-save.png");

fn main() -> eframe::Result {
    let mut args = std::env::args_os().skip(1);
    let first = args.next();
    if first.as_deref() == Some(std::ffi::OsStr::new("--offline-smoke-test")) {
        let result = match (args.next(), args.next(), args.next()) {
            (Some(input), Some(output), None) => smoke_test(input.into(), &PathBuf::from(output)),
            _ => Err("Expected an input photo and output PNG path".into()),
        };
        if let Err(error) = result {
            eprintln!("{error:#}");
            std::process::exit(1);
        }
        return Ok(());
    }

    let icon = image::load_from_memory(ICON)
        .expect("embedded app icon")
        .into_rgba8();
    let (width, height) = icon.dimensions();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([960.0, 680.0])
            .with_min_inner_size([620.0, 430.0])
            .with_drag_and_drop(true)
            .with_icon(egui::IconData {
                rgba: icon.into_raw(),
                width,
                height,
            }),
        ..Default::default()
    };
    eframe::run_native(
        NAME,
        options,
        Box::new(|creation| {
            creation.egui_ctx.set_visuals(egui::Visuals::dark());
            let mut app = App::default();
            if let Some(path) = first {
                app.open(&creation.egui_ctx, path.into());
            }
            Ok(Box::new(app))
        }),
    )
}

// Exercises the shipped binary without needing a desktop session or any external model file.
fn smoke_test(input: PathBuf, output: &Path) -> Result<(), Error> {
    let photo = Photo::load(input)?;
    if photo.regions.is_empty() {
        return Err("The bundled model did not detect a face.".into());
    }
    let blurred = photo::render(&photo.source, &photo.regions, Effect::Blur);
    if *photo.source == blurred {
        return Err("Blurring did not change any pixels.".into());
    }
    photo::save_png(blurred, output)
}

struct Photo {
    path: PathBuf,
    source: Arc<RgbaImage>,
    preview: RgbaImage,
    regions: Vec<Region>,
    detection_failed: bool,
}

impl Photo {
    fn load(path: PathBuf) -> Result<Self, Error> {
        let source = Arc::new(photo::open(&path)?);
        let preview = photo::preview(&source);
        let (regions, detection_failed) =
            match Detector::new().and_then(|detector| detector.faces(&preview)) {
                Ok(regions) => (regions, false),
                Err(error) => {
                    eprintln!("Face detection failed: {error:#}");
                    (Vec::new(), true)
                }
            };
        Ok(Self {
            path,
            source,
            preview,
            regions,
            detection_failed,
        })
    }
}

enum Event {
    Opened(Result<Photo, Error>),
    Saved(Result<PathBuf, Error>),
}

#[derive(Default)]
struct App {
    photo: Option<Photo>,
    texture: Option<TextureHandle>,
    effect: Effect,
    pending: Option<mpsc::Receiver<Event>>,
    status: String,
    drag: Option<[f32; 2]>,
    show_licenses: bool,
    show_original: bool,
}

impl App {
    fn job(
        &mut self,
        ctx: &egui::Context,
        status: &str,
        work: impl FnOnce() -> Event + Send + 'static,
    ) {
        let (send, receive) = mpsc::channel();
        let ctx = ctx.clone();
        self.pending = Some(receive);
        self.status = status.into();
        thread::spawn(move || {
            let _ = send.send(work());
            ctx.request_repaint();
        });
    }

    fn open(&mut self, ctx: &egui::Context, path: PathBuf) {
        self.job(ctx, "Opening photo and looking for faces…", move || {
            Event::Opened(Photo::load(path))
        });
    }

    fn save(&mut self, ctx: &egui::Context) {
        let Some(photo) = self.photo.as_ref() else {
            return;
        };
        let stem = photo.path.file_stem().unwrap_or_default().to_string_lossy();
        let mut dialog = rfd::FileDialog::new()
            .set_title("Save a copy")
            .add_filter("PNG image", &["png"])
            .set_file_name(format!("{stem}-blurred.png"));
        if let Some(parent) = photo.path.parent() {
            dialog = dialog.set_directory(parent);
        }
        let Some(mut path) = dialog.save_file() else {
            return;
        };
        path.set_extension("png");
        let source = photo.source.clone();
        let regions = photo.regions.clone();
        let effect = self.effect;
        self.job(ctx, "Saving a copy…", move || {
            Event::Saved(
                photo::save_png(photo::render(&source, &regions, effect), &path).map(|()| path),
            )
        });
    }

    fn update_job(&mut self, ctx: &egui::Context) {
        let result = self.pending.as_ref().map(mpsc::Receiver::try_recv);
        match result {
            Some(Ok(Event::Opened(Ok(photo)))) => {
                self.pending = None;
                self.status = if photo.detection_failed {
                    "Automatic detection was unavailable. Drag over faces to mark them.".into()
                } else if photo.regions.is_empty() {
                    "No faces found. Drag over any faces you want to hide.".into()
                } else {
                    format!(
                        "Found {} face(s). Review the photo before saving.",
                        photo.regions.len()
                    )
                };
                self.drag = None;
                self.show_original = false;
                self.photo = Some(photo);
                self.refresh_preview(ctx);
            }
            Some(Ok(Event::Saved(Ok(path)))) => {
                self.pending = None;
                self.status = format!("Saved {}", filename(&path));
            }
            Some(Ok(Event::Opened(Err(error)))) => {
                self.pending = None;
                self.status = format!("Couldn’t open the photo. {error}");
            }
            Some(Ok(Event::Saved(Err(error)))) => {
                self.pending = None;
                self.status = format!("Couldn’t save the copy. {error}");
            }
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                self.pending = None;
                self.status = "Processing stopped unexpectedly. Please try again.".into();
            }
            _ => {}
        }
    }

    fn refresh_preview(&mut self, ctx: &egui::Context) {
        let Some(photo) = &self.photo else { return };
        let preview = if self.show_original {
            photo.preview.clone()
        } else {
            photo::render(&photo.preview, &photo.regions, self.effect)
        };
        let pixels = ColorImage::from_rgba_unmultiplied(
            [preview.width() as usize, preview.height() as usize],
            preview.as_raw(),
        );
        match &mut self.texture {
            Some(texture) => texture.set(pixels, egui::TextureOptions::LINEAR),
            None => {
                self.texture = Some(ctx.load_texture("photo", pixels, egui::TextureOptions::LINEAR))
            }
        }
    }

    fn tools(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let busy = self.pending.is_some();
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(!busy, egui::Button::new("Open photo…"))
                .clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .set_title("Open a photo")
                    .add_filter("Photos", &["jpg", "jpeg", "png", "webp"])
                    .pick_file()
            {
                self.open(&ctx, path);
            }

            let can_save = self
                .photo
                .as_ref()
                .is_some_and(|photo| !photo.regions.is_empty());
            if ui
                .add_enabled(
                    !busy && can_save,
                    egui::Button::new("Save a copy…").fill(Color32::from_rgb(16, 104, 97)),
                )
                .clicked()
            {
                self.save(&ctx);
            }

            ui.separator();
            ui.add_enabled_ui(!busy && self.photo.is_some(), |ui| {
                let blur_changed = ui
                    .selectable_value(&mut self.effect, Effect::Blur, "Blur")
                    .changed();
                let cover_changed = ui
                    .selectable_value(&mut self.effect, Effect::Cover, "Black boxes")
                    .changed();
                let original_changed = ui
                    .checkbox(&mut self.show_original, "Show original")
                    .changed();
                if blur_changed || cover_changed || original_changed {
                    self.refresh_preview(&ctx);
                }
            });
            if self.pending.is_some() {
                ui.spinner();
            }
        });
    }

    fn canvas(&mut self, ui: &mut egui::Ui) {
        let available = ui.available_size();
        let (canvas, _) = ui.allocate_exact_size(available, Sense::hover());
        let painter = ui.painter_at(canvas);
        painter.rect_filled(canvas, 12, Color32::from_rgb(17, 24, 35));

        let Some(texture) = &self.texture else {
            painter.text(
                canvas.center() - Vec2::new(0., 14.),
                egui::Align2::CENTER_CENTER,
                "Drop a photo here",
                egui::FontId::proportional(26.),
                Color32::from_rgb(226, 232, 240),
            );
            painter.text(
                canvas.center() + Vec2::new(0., 20.),
                egui::Align2::CENTER_CENTER,
                "JPG, PNG or WebP",
                egui::FontId::proportional(15.),
                Color32::from_rgb(150, 166, 184),
            );
            return;
        };
        let size = texture.size_vec2();
        let scale =
            ((canvas.width() - 20.).max(1.) / size.x).min((canvas.height() - 20.).max(1.) / size.y);
        let rect = Rect::from_center_size(canvas.center(), size * scale);
        painter.image(
            texture.id(),
            rect,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
            Color32::WHITE,
        );

        let response = ui.interact(rect, ui.id().with("edit-photo"), Sense::click_and_drag());
        let busy = self.pending.is_some();
        let mut changed = false;
        if let Some(photo) = &mut self.photo {
            let local = |position: Pos2| {
                [
                    (position.x - rect.min.x) / rect.width(),
                    (position.y - rect.min.y) / rect.height(),
                ]
            };
            let screen = |point: [f32; 2]| {
                Pos2::new(
                    rect.min.x + point[0] * rect.width(),
                    rect.min.y + point[1] * rect.height(),
                )
            };
            let cursor = response
                .interact_pointer_pos()
                .or_else(|| ui.input(|input| input.pointer.latest_pos()));
            if !busy && response.drag_started_by(egui::PointerButton::Primary) {
                self.drag = ui.input(|input| input.pointer.press_origin()).map(local);
            }
            if let (Some(start), Some(end)) = (self.drag, cursor) {
                let selection = Region::new(start, local(end));
                let drawn = Rect::from_min_max(screen(selection.min), screen(selection.max));
                painter.rect_stroke(
                    drawn,
                    2,
                    egui::Stroke::new(2., Color32::from_rgb(83, 233, 205)),
                    egui::StrokeKind::Inside,
                );
                if response.drag_stopped_by(egui::PointerButton::Primary) {
                    if drawn.width() >= 5. && drawn.height() >= 5. {
                        photo.regions.push(selection);
                        changed = true;
                    }
                    self.drag = None;
                }
            }
            if !ui.input(|input| input.pointer.primary_down()) {
                self.drag = None;
            }
            if !busy
                && response.secondary_clicked()
                && let Some(index) = cursor.and_then(|point| {
                    photo
                        .regions
                        .iter()
                        .rposition(|region| region.contains(local(point)))
                })
            {
                photo.regions.remove(index);
                changed = true;
            }
            if self.show_original {
                for region in &photo.regions {
                    painter.rect_stroke(
                        Rect::from_min_max(screen(region.min), screen(region.max)),
                        2,
                        egui::Stroke::new(2., Color32::from_rgb(83, 233, 205)),
                        egui::StrokeKind::Inside,
                    );
                }
            }
        }
        if changed {
            self.show_original = false;
            self.refresh_preview(ui.ctx());
            self.status = "Review the photo, then save a copy.".into();
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.update_job(ctx);
        if self.pending.is_none()
            && let Some(path) = ctx.input(|input| {
                input
                    .raw
                    .dropped_files
                    .iter()
                    .find_map(|file| file.path.clone())
            })
        {
            self.open(ctx, path);
        }

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.add_space(8.);
            ui.heading(NAME);
            ui.add_space(6.);
            self.tools(ui);
            ui.add_space(8.);
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.add_space(5.);
            if !self.status.is_empty() {
                ui.label(&self.status);
            }
            ui.horizontal_wrapped(|ui| {
                if let Some(photo) = &self.photo {
                    ui.weak(filename(&photo.path));
                    ui.separator();
                    ui.weak("Drag to add a box. Right-click a box to remove it.");
                    ui.separator();
                }
                if ui.link("Licenses").clicked() {
                    self.show_licenses = true;
                }
            });
            ui.add_space(5.);
        });

        egui::CentralPanel::default().show(ctx, |ui| self.canvas(ui));
        egui::Window::new("Licenses")
            .open(&mut self.show_licenses)
            .default_width(540.)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(360.)
                    .show(ui, |ui| {
                        ui.heading(NAME);
                        ui.label(include_str!("../LICENSE"));
                        ui.separator();
                        ui.heading("UltraFace");
                        ui.label(include_str!("../assets/ULTRAFACE-LICENSE.txt"));
                        ui.hyperlink_to(
                            "Third-party notices",
                            "https://github.com/traversaro/blur-and-save/releases",
                        );
                    });
            });
    }
}

fn filename(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

use std::path::Path;

use image::{ImageDecoder, ImageFormat, ImageReader, Rgba, RgbaImage, imageops};

pub(crate) type Error = Box<dyn std::error::Error + Send + Sync>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Region {
    pub min: [f32; 2],
    pub max: [f32; 2],
}

impl Region {
    pub fn new(a: [f32; 2], b: [f32; 2]) -> Self {
        Self {
            min: [a[0].min(b[0]).clamp(0., 1.), a[1].min(b[1]).clamp(0., 1.)],
            max: [a[0].max(b[0]).clamp(0., 1.), a[1].max(b[1]).clamp(0., 1.)],
        }
    }

    pub fn size(self) -> [f32; 2] {
        [self.max[0] - self.min[0], self.max[1] - self.min[1]]
    }

    pub fn padded(self, fraction: f32) -> Self {
        let [width, height] = self.size();
        Self::new(
            [
                self.min[0] - width * fraction,
                self.min[1] - height * fraction,
            ],
            [
                self.max[0] + width * fraction,
                self.max[1] + height * fraction,
            ],
        )
    }

    pub fn contains(self, point: [f32; 2]) -> bool {
        (0..2).all(|axis| (self.min[axis]..=self.max[axis]).contains(&point[axis]))
    }

    pub fn iou(self, other: Self) -> f32 {
        let overlap = (0..2)
            .map(|axis| {
                (self.max[axis].min(other.max[axis]) - self.min[axis].max(other.min[axis])).max(0.)
            })
            .product::<f32>();
        let area = |region: Self| region.size().into_iter().product::<f32>();
        overlap / (area(self) + area(other) - overlap).max(f32::EPSILON)
    }
}

#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) enum Effect {
    #[default]
    Blur,
    Cover,
}

pub(crate) fn open(path: &Path) -> Result<RgbaImage, Error> {
    let mut reader = ImageReader::open(path)?.with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(24_000);
    limits.max_image_height = Some(24_000);
    reader.limits(limits);
    let mut decoder = reader.into_decoder()?;
    let (width, height) = decoder.dimensions();
    if u64::from(width) * u64::from(height) > 50_000_000 {
        return Err("This photo is larger than 50 megapixels.".into());
    }
    let orientation = decoder.orientation()?;
    let mut image = image::DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    Ok(image.into_rgba8())
}

pub(crate) fn preview(source: &RgbaImage) -> RgbaImage {
    let longest = source.width().max(source.height());
    if longest <= 1440 {
        return source.clone();
    }
    let scale = 1440.0 / f64::from(longest);
    let scaled = |size| (f64::from(size) * scale).round().max(1.0) as u32;
    imageops::resize(
        source,
        scaled(source.width()),
        scaled(source.height()),
        imageops::FilterType::Triangle,
    )
}

pub(crate) fn render(source: &RgbaImage, regions: &[Region], effect: Effect) -> RgbaImage {
    let mut output = source.clone();
    for region in regions {
        let bounds = |axis: usize, length: u32| {
            let scale = length as f32;
            let start = (region.min[axis] * scale).floor() as u32;
            let end = ((region.max[axis] * scale).ceil() as u32).min(length);
            (start, end - start)
        };
        let (x, width) = bounds(0, source.width());
        let (y, height) = bounds(1, source.height());
        if width == 0 || height == 0 {
            continue;
        }
        let patch = match effect {
            Effect::Blur => {
                let crop = imageops::crop_imm(&output, x, y, width, height).to_image();
                imageops::fast_blur(&crop, (width.min(height) as f32 * 0.22).max(1.0))
            }
            Effect::Cover => RgbaImage::from_pixel(width, height, Rgba([0, 0, 0, 255])),
        };
        imageops::replace(&mut output, &patch, i64::from(x), i64::from(y));
    }
    output
}

pub(crate) fn save_png(output: RgbaImage, path: &Path) -> Result<(), Error> {
    let dir = path
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(dir)?;
    image::DynamicImage::ImageRgba8(output).write_to(temporary.as_file_mut(), ImageFormat::Png)?;
    temporary.persist_noclobber(path).map_err(|error| {
        if error.error.kind() == std::io::ErrorKind::AlreadyExists {
            "That file already exists. Choose a new name.".into()
        } else {
            Box::new(error.error) as Error
        }
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_changes_only_the_selected_pixels() {
        let white = Rgba([255; 4]);
        let photo = RgbaImage::from_pixel(10, 10, white);
        let covered = render(
            &photo,
            &[Region::new([0.8, 0.8], [0.2, 0.2])],
            Effect::Cover,
        );
        assert_eq!(*covered.get_pixel(9, 9), white);
        assert_eq!(*covered.get_pixel(3, 3), Rgba([0, 0, 0, 255]));
        assert_eq!(*photo.get_pixel(3, 3), white);
    }

    #[test]
    fn blur_leaves_every_pixel_outside_the_selection_identical() {
        let input = RgbaImage::from_fn(100, 80, |x, y| {
            let shade = if (x + y) % 2 == 0 { 0 } else { 255 };
            Rgba([shade, shade, shade, 255])
        });
        let blurred = render(
            &input,
            &[Region::new([0.25, 0.25], [0.75, 0.75])],
            Effect::Blur,
        );
        assert_ne!(blurred.get_pixel(50, 40), input.get_pixel(50, 40));
        for (x, y, pixel) in blurred.enumerate_pixels() {
            if !(25..75).contains(&x) || !(20..60).contains(&y) {
                assert_eq!(pixel, input.get_pixel(x, y), "pixel ({x}, {y})");
            }
        }
    }

    #[test]
    fn saving_preserves_existing_files() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("copy.png");
        let photo = RgbaImage::from_pixel(1, 1, Rgba([255; 4]));
        save_png(photo.clone(), &path).unwrap();
        assert!(save_png(RgbaImage::new(1, 1), &path).is_err());
        assert_eq!(open(&path).unwrap(), photo);
    }
}

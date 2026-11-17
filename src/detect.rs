use image::{RgbaImage, imageops};
use tract_onnx::prelude::*;

use crate::photo::{Error, Region};

const WIDTH: usize = 640;
const HEIGHT: usize = 480;
const SCORE_THRESHOLD: f32 = 0.55;
const MODEL: &[u8] = include_bytes!("../assets/ultraface-rfb-640.onnx");

pub(crate) struct Detector(TypedRunnableModel<TypedModel>);

impl Detector {
    pub fn new() -> Result<Self, Error> {
        let model = tract_onnx::onnx()
            .model_for_read(&mut std::io::Cursor::new(MODEL))?
            .with_input_fact(
                0,
                InferenceFact::dt_shape(f32::datum_type(), [1, 3, HEIGHT, WIDTH]),
            )?
            .into_optimized()?
            .into_runnable()?;
        Ok(Self(model))
    }

    pub fn faces(&self, photo: &RgbaImage) -> Result<Vec<Region>, Error> {
        let resized = imageops::resize(
            photo,
            WIDTH as u32,
            HEIGHT as u32,
            imageops::FilterType::Triangle,
        );
        let input =
            tract_ndarray::Array4::from_shape_fn([1, 3, HEIGHT, WIDTH], |(_, channel, y, x)| {
                let pixel = resized.get_pixel(x as u32, y as u32);
                let alpha = f32::from(pixel[3]) / 255.0;
                let rgb = f32::from(pixel[channel]) * alpha + 127.0 * (1.0 - alpha);
                (rgb - 127.0) / 128.0
            });
        let output = self.0.run(tvec!(input.into_tensor().into()))?;

        // The final axis distinguishes the class scores and the normalized XYXY boxes.
        let scores = output
            .iter()
            .find(|tensor| tensor.shape().last() == Some(&2))
            .ok_or("The face model did not return scores.")?
            .as_slice::<f32>()?;
        let boxes = output
            .iter()
            .find(|tensor| tensor.shape().last() == Some(&4))
            .ok_or("The face model did not return boxes.")?
            .as_slice::<f32>()?;
        if scores.len() / 2 != boxes.len() / 4 {
            return Err("The face model returned mismatched scores and boxes.".into());
        }

        let mut candidates: Vec<_> = scores
            .chunks_exact(2)
            .zip(boxes.chunks_exact(4))
            .filter(|(score, xyxy)| {
                score[1] >= SCORE_THRESHOLD
                    && xyxy.iter().all(|coordinate| coordinate.is_finite())
                    && xyxy[0] < xyxy[2]
                    && xyxy[1] < xyxy[3]
            })
            .map(|(score, box_)| {
                (
                    score[1],
                    Region::new([box_[0], box_[1]], [box_[2], box_[3]]),
                )
            })
            .filter(|(_, region)| region.size().into_iter().all(|side| side > 0.))
            .collect();
        candidates.sort_unstable_by(|a, b| b.0.total_cmp(&a.0));

        let mut selected = Vec::new();
        for (_, candidate) in candidates {
            if selected.iter().all(|region| candidate.iou(*region) <= 0.3) {
                selected.push(candidate);
            }
        }
        Ok(selected
            .into_iter()
            .map(|region| region.padded(0.24))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_detector_finds_a_face_and_ignores_a_blank_image() {
        let detector = Detector::new().unwrap();
        let portrait = image::load_from_memory(include_bytes!("../tests/fixtures/astronaut.jpg"))
            .unwrap()
            .into_rgba8();
        assert!(
            detector
                .faces(&portrait)
                .unwrap()
                .iter()
                .any(|face| face.contains([0.44, 0.24]))
        );
        let blank = RgbaImage::from_pixel(640, 480, image::Rgba([127, 127, 127, 255]));
        assert!(detector.faces(&blank).unwrap().is_empty());
    }
}

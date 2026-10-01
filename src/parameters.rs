use serde::Deserialize;

use crate::error::json_error;
use crate::Result;

// vvdp_data/cvvdp_parameters.json; unused upstream keys are ignored.
#[derive(Deserialize)]
pub(crate) struct Parameters {
    pub mask_p: f32,
    pub mask_c: f32,
    pub pu_dilate: f32,
    pub beta: f32,
    pub beta_t: f32,
    pub beta_tch: f32,
    pub beta_sch: f32,
    pub sensitivity_correction: f32,
    pub jod_a: f32,
    pub jod_exp: f32,
    pub mask_q: [f32; 4],
    pub ch_chrom_w: f32,
    pub ch_trans_w: f32,
    pub sigma_tf: [f32; 4],
    pub beta_tf: [f32; 4],
    pub xcm_weights: [f32; 16],
    pub baseband_weight: [f32; 4],
    pub d_max: f32,
    pub image_int: f32,
}

impl Parameters {
    pub fn load() -> Result<Self> {
        serde_json::from_str(include_str!("../data/cvvdp_parameters.json")).map_err(json_error)
    }

    pub fn channel_weights(&self) -> [f32; 4] {
        [1.0, self.ch_chrom_w, self.ch_chrom_w, self.ch_trans_w]
    }

    // cvvdp_metric.py: met2jod, including the linear segment below Q=0.1.
    pub fn jod(&self, q: f32) -> f32 {
        if q <= 0.1 {
            10.0 - self.jod_a * 0.1_f32.powf(self.jod_exp - 1.0) * q
        } else {
            10.0 - self.jod_a * q.powf(self.jod_exp)
        }
    }
}

// cvvdp_metric.py: safe_pow and lp_norm. The epsilon applies at both powers.
pub(crate) fn safe_pow(x: f32, p: f32) -> f32 {
    (x + 0.00001).powf(p) - 0.00001_f32.powf(p)
}

pub(crate) fn norm(values: impl Iterator<Item = f32>, p: f32, count: usize) -> f32 {
    let sum: f64 = values.map(|x| safe_pow(x, p) as f64).sum();
    safe_pow((sum / count as f64) as f32, 1.0 / p)
}

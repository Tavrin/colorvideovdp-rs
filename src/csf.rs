use serde::Deserialize;

use crate::allocation::collect;
use crate::error::json_error;
use crate::{Error, Result};

#[derive(Deserialize)]
struct Lut {
    #[serde(rename = "L_bkg")]
    luminance: Vec<f32>,
    rho: Vec<f32>,
    o0_c1: Vec<Vec<f32>>,
    o0_c2: Vec<Vec<f32>>,
    o0_c3: Vec<Vec<f32>>,
    o5_c1: Vec<Vec<f32>>,
}

pub(crate) struct Csf {
    log_l: Vec<f32>,
    log_rho: Vec<f32>,
    tables: [Vec<Vec<f32>>; 4],
}

impl Csf {
    pub fn load() -> Result<Self> {
        let lut: Lut = serde_json::from_str(include_str!("../data/csf_lut_weber_fixed_size.json"))
            .map_err(json_error)?;
        let tables = [lut.o0_c1, lut.o0_c2, lut.o0_c3, lut.o5_c1];
        if lut.luminance.len() < 2
            || lut.rho.len() < 2
            || tables.iter().any(|t| {
                t.len() != lut.luminance.len() || t.iter().any(|r| r.len() != lut.rho.len())
            })
        {
            return Err(Error::InvalidConfig("invalid CSF table shape".into()));
        }
        Ok(Self {
            log_l: lut.luminance.iter().map(|x| x.log10()).collect(),
            log_rho: lut.rho.iter().map(|x| x.log10()).collect(),
            tables,
        })
    }

    // csf.py: castleCSF.sensitivity; batch_interp1d extrapolates spatial frequency.
    pub fn at_frequency(&self, frequency: f32) -> Result<[Vec<f32>; 4]> {
        let query = frequency.log10();
        let hi = self
            .log_rho
            .partition_point(|&x| x < query)
            .clamp(1, self.log_rho.len() - 1);
        let lo = hi - 1;
        let channels = collect((0..4).map(|c| {
            collect(self.tables[c].iter().map(|r| {
                let slope = (r[hi] - r[lo]) / (self.log_rho[hi] - self.log_rho[lo]);
                Ok(r[lo] + slope * (query - self.log_rho[lo]))
            }))
        }))?;
        channels.try_into().map_err(|_| Error::NumericalFailure)
    }

    // interp.py: interp1q clamps on a uniformly spaced log-luminance axis.
    pub fn sensitivity(&self, table: &[f32], log_l: f32) -> f32 {
        let last = self.log_l.len() - 1;
        let index = ((log_l - self.log_l[0]) / (self.log_l[last] - self.log_l[0]) * last as f32)
            .clamp(0.0, last as f32);
        let lo = index as usize;
        let hi = (lo + 1).min(last);
        let f = index.fract();
        10.0_f32.powf(table[lo] * (1.0 - f) + table[hi] * f)
    }
}

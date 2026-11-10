//! A radix-2 FFT and a spectrum analyser that maps a block of samples onto log-spaced bands
//! (what the plugin's analyser draws).

use std::f32::consts::PI;

/// Radix-2 complex FFT of a fixed power-of-two size.
#[derive(Debug, Clone)]
pub struct Fft {
    n: usize,
    cos: Vec<f32>,
    sin: Vec<f32>,
    rev: Vec<u32>,
}

impl Fft {
    /// An FFT of `n` points (`n` a power of two).
    pub fn new(n: usize) -> Self {
        assert!(n.is_power_of_two() && n >= 4);
        let bits = n.trailing_zeros();
        let rev = (0..n as u32)
            .map(|i| i.reverse_bits() >> (32 - bits))
            .collect();
        let cos = (0..n / 2)
            .map(|k| (2.0 * PI * k as f32 / n as f32).cos())
            .collect();
        let sin = (0..n / 2)
            .map(|k| -(2.0 * PI * k as f32 / n as f32).sin())
            .collect();
        Self { n, cos, sin, rev }
    }
    /// In-place forward transform.
    pub fn forward(&self, re: &mut [f32], im: &mut [f32]) {
        let n = self.n;
        for i in 0..n {
            let j = self.rev[i] as usize;
            if j > i {
                re.swap(i, j);
                im.swap(i, j);
            }
        }
        let mut len = 2;
        while len <= n {
            let half = len / 2;
            let step = n / len;
            let mut start = 0;
            while start < n {
                for k in 0..half {
                    let (wr, wi) = (self.cos[k * step], self.sin[k * step]);
                    let (a, b) = (start + k, start + k + half);
                    let tr = re[b] * wr - im[b] * wi;
                    let ti = re[b] * wi + im[b] * wr;
                    re[b] = re[a] - tr;
                    im[b] = im[a] - ti;
                    re[a] += tr;
                    im[a] += ti;
                }
                start += len;
            }
            len <<= 1;
        }
    }
}

/// Hann-windowed magnitude spectrum, 0 dB = a full-scale sine.
#[derive(Debug, Clone)]
pub struct SpectrumAnalyzer {
    fft: Fft,
    window: Vec<f32>,
    norm: f32,
    re: Vec<f32>,
    im: Vec<f32>,
    /// Magnitude in dBFS for bins 0..n/2.
    pub db: Vec<f32>,
}

impl SpectrumAnalyzer {
    /// An analyser of `n` samples.
    pub fn new(n: usize) -> Self {
        let window: Vec<f32> = (0..n)
            .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / n as f32).cos())
            .collect();
        let norm = 2.0 / window.iter().sum::<f32>();
        Self {
            fft: Fft::new(n),
            window,
            norm,
            re: vec![0.0; n],
            im: vec![0.0; n],
            db: vec![-120.0; n / 2],
        }
    }
    /// Block length.
    pub fn len(&self) -> usize {
        self.window.len()
    }
    /// Always false (an analyser has a length).
    pub fn is_empty(&self) -> bool {
        false
    }
    /// Analyse the last `len()` samples of `x` (shorter input is zero-padded at the front).
    pub fn analyze(&mut self, x: &[f32]) {
        let n = self.len();
        let take = x.len().min(n);
        let src = &x[x.len() - take..];
        self.re.fill(0.0);
        self.im.fill(0.0);
        for (i, v) in src.iter().enumerate() {
            let k = n - take + i;
            self.re[k] = v * self.window[k];
        }
        self.fft.forward(&mut self.re, &mut self.im);
        for k in 0..n / 2 {
            let m = (self.re[k] * self.re[k] + self.im[k] * self.im[k]).sqrt() * self.norm;
            self.db[k] = 20.0 * m.max(1.0e-7).log10();
        }
    }
    /// Frequency of the loudest bin (parabolic interpolation).
    pub fn peak_frequency(&self, sr: f64) -> f32 {
        let n = self.len();
        let (mut k, mut best) = (1, f32::MIN);
        for i in 1..n / 2 - 1 {
            if self.db[i] > best {
                best = self.db[i];
                k = i;
            }
        }
        let (a, b, c) = (self.db[k - 1], self.db[k], self.db[k + 1]);
        let denom = a - 2.0 * b + c;
        let off = if denom.abs() > 1e-9 {
            0.5 * (a - c) / denom
        } else {
            0.0
        };
        ((k as f32 + off) as f64 * sr / n as f64) as f32
    }
    /// Map the last analysis onto `out.len()` bands, log-spaced from `f_lo` to `f_hi` Hz.
    /// Each band takes the loudest bin inside it, or an interpolated value where the band is
    /// narrower than a bin (the low end).
    pub fn log_bands(&self, sr: f64, f_lo: f32, f_hi: f32, out: &mut [f32]) {
        let n = self.len();
        let bins = n / 2;
        let bin_hz = sr as f32 / n as f32;
        let nb = out.len();
        let ratio = (f_hi / f_lo).powf(1.0 / nb as f32);
        let mut lo = f_lo;
        for o in out.iter_mut() {
            let hi = lo * ratio;
            let k0 = (lo / bin_hz).ceil() as usize;
            let k1 = ((hi / bin_hz).floor() as usize).min(bins - 1);
            *o = if k1 >= k0 && k0 >= 1 {
                self.db[k0..=k1].iter().cloned().fold(f32::MIN, f32::max)
            } else {
                let c = (lo * hi).sqrt() / bin_hz;
                let i = (c.floor() as usize).min(bins - 2);
                let f = c - i as f32;
                self.db[i] * (1.0 - f) + self.db[i + 1] * f
            };
            lo = hi;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(hz: f32, amp: f32, sr: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| amp * (2.0 * PI * hz * i as f32 / sr).sin())
            .collect()
    }

    #[test]
    fn fft_of_an_impulse_is_flat() {
        let f = Fft::new(64);
        let mut re = vec![0.0; 64];
        let mut im = vec![0.0; 64];
        re[0] = 1.0;
        f.forward(&mut re, &mut im);
        assert!(re.iter().all(|v| (v - 1.0).abs() < 1e-6) && im.iter().all(|v| v.abs() < 1e-6));
    }

    #[test]
    fn fft_matches_a_direct_dft() {
        let n = 32;
        let x: Vec<f32> = (0..n).map(|i| ((i * 7 % 11) as f32 - 5.0) / 5.0).collect();
        let f = Fft::new(n);
        let (mut re, mut im) = (x.clone(), vec![0.0; n]);
        f.forward(&mut re, &mut im);
        for k in 0..n {
            let (mut sr, mut si) = (0.0f64, 0.0f64);
            for (t, v) in x.iter().enumerate() {
                let a = -2.0 * std::f64::consts::PI * (k * t) as f64 / n as f64;
                sr += *v as f64 * a.cos();
                si += *v as f64 * a.sin();
            }
            assert!(
                (re[k] as f64 - sr).abs() < 1e-4 && (im[k] as f64 - si).abs() < 1e-4,
                "bin {k}"
            );
        }
    }

    #[test]
    fn a_full_scale_sine_reads_zero_dbfs_and_is_found() {
        let mut an = SpectrumAnalyzer::new(4096);
        an.analyze(&sine(1000.0, 1.0, 48_000.0, 4096));
        let peak = an.db.iter().cloned().fold(f32::MIN, f32::max);
        assert!(peak > -1.5 && peak < 0.5, "{peak}");
        assert!((an.peak_frequency(48_000.0) - 1000.0).abs() < 5.0);
    }

    #[test]
    fn log_bands_put_the_tone_in_the_right_band() {
        let mut an = SpectrumAnalyzer::new(4096);
        an.analyze(&sine(1000.0, 0.5, 48_000.0, 4096));
        let mut bands = vec![0.0; 256];
        an.log_bands(48_000.0, 20.0, 20_000.0, &mut bands);
        let (idx, v) = bands
            .iter()
            .cloned()
            .enumerate()
            .fold((0, f32::MIN), |a, (i, v)| if v > a.1 { (i, v) } else { a });
        let centre = 20.0 * 1000f32.powf((idx as f32 + 0.5) / 256.0);
        assert!((centre / 1000.0 - 1.0).abs() < 0.05, "{centre}");
        assert!((v + 6.0).abs() < 1.5, "{v}");
        assert!(bands[10] < -60.0 && bands[250] < -60.0);
        assert!(bands.iter().all(|b| b.is_finite()));
    }

    #[test]
    fn silence_floors_instead_of_going_to_minus_infinity() {
        let mut an = SpectrumAnalyzer::new(1024);
        an.analyze(&[0.0; 1024]);
        assert!(an.db.iter().all(|d| d.is_finite() && *d <= -139.0));
    }
}

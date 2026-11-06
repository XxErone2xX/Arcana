//! 아주 작은 텐서 라이브러리 (외부 라이브러리 없이 직접 구현)
//! 나중에 SIMD, 멀티스레드, GPU, 양자화(int8) 같은 걸 붙일 때 여기만 바꾸면 됨.

/// 2차원 행렬 (행 우선 저장)
#[derive(Clone, Debug)]
pub struct Mat {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<f32>,
}

impl Mat {
    pub fn zeros(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            data: vec![0.0; rows * cols],
        }
    }

    pub fn row(&self, r: usize) -> &[f32] {
        &self.data[r * self.cols..(r + 1) * self.cols]
    }

    pub fn row_mut(&mut self, r: usize) -> &mut [f32] {
        &mut self.data[r * self.cols..(r + 1) * self.cols]
    }

    pub fn add_assign(&mut self, other: &Mat) {
        debug_assert_eq!(self.data.len(), other.data.len());
        for (a, b) in self.data.iter_mut().zip(&other.data) {
            *a += *b;
        }
    }
}

/// 선형층 y = x·Wᵀ + b   (PyTorch nn.Linear와 같은 배치: W는 [out, in])
pub struct Linear {
    pub weight: Mat, // [out, in]
    pub bias: Vec<f32>,
}

impl Linear {
    pub fn forward(&self, x: &Mat) -> Mat {
        let (out_dim, in_dim) = (self.weight.rows, self.weight.cols);
        assert_eq!(x.cols, in_dim, "Linear 입력 크기가 안 맞음");
        let mut y = Mat::zeros(x.rows, out_dim);
        for t in 0..x.rows {
            let xr = x.row(t);
            let yr = y.row_mut(t);
            for o in 0..out_dim {
                yr[o] = dot(xr, self.weight.row(o)) + self.bias[o];
            }
        }
        y
    }
}

#[inline]
pub fn dot(a: &[f32], b: &[f32]) -> f32 {
    // 4개씩 묶어서 더하면 컴파일러가 SIMD로 바꿔주기 쉬움
    let mut s = [0.0f32; 4];
    let chunks = a.len() / 4;
    for i in 0..chunks {
        let j = i * 4;
        s[0] += a[j] * b[j];
        s[1] += a[j + 1] * b[j + 1];
        s[2] += a[j + 2] * b[j + 2];
        s[3] += a[j + 3] * b[j + 3];
    }
    let mut total = s[0] + s[1] + s[2] + s[3];
    for j in chunks * 4..a.len() {
        total += a[j] * b[j];
    }
    total
}

pub struct LayerNorm {
    pub weight: Vec<f32>,
    pub bias: Vec<f32>,
    pub eps: f32,
}

impl LayerNorm {
    pub fn forward(&self, x: &Mat) -> Mat {
        let mut y = x.clone();
        let n = x.cols as f32;
        for t in 0..x.rows {
            let r = y.row_mut(t);
            let mean = r.iter().sum::<f32>() / n;
            let var = r.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / n;
            let inv = 1.0 / (var + self.eps).sqrt();
            for (i, v) in r.iter_mut().enumerate() {
                *v = (*v - mean) * inv * self.weight[i] + self.bias[i];
            }
        }
        y
    }
}

/// GELU (PyTorch 기본값과 같은 erf 버전)
pub fn gelu_inplace(x: &mut Mat) {
    for v in x.data.iter_mut() {
        *v = 0.5 * *v * (1.0 + erf(*v / std::f32::consts::SQRT_2));
    }
}

/// 오차함수 erf (Abramowitz–Stegun 7.1.26 고정밀 근사, 오차 < 1.5e-7)
pub fn erf(x: f32) -> f32 {
    let x64 = x as f64;
    let sign = if x64 < 0.0 { -1.0 } else { 1.0 };
    let a = x64.abs();
    let t = 1.0 / (1.0 + 0.3275911 * a);
    let y = 1.0
        - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t
            + 0.254829592)
            * t
            * (-a * a).exp();
    (sign * y) as f32
}

pub fn softmax_inplace(v: &mut [f32]) {
    let m = v.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let mut s = 0.0;
    for x in v.iter_mut() {
        *x = (*x - m).exp();
        s += *x;
    }
    for x in v.iter_mut() {
        *x /= s;
    }
}

pub fn argmax(v: &[f32]) -> usize {
    let mut best = 0;
    for i in 1..v.len() {
        if v[i] > v[best] {
            best = i;
        }
    }
    best
}

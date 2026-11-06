//! safetensors 파일 읽기 (HuggingFace 표준 형식, 직접 파싱)
//! 구조: [헤더 길이 u64 LE][JSON 헤더][원본 바이트들]

use crate::tensor::Mat;
use crate::Error;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
struct Entry {
    dtype: String,
    shape: Vec<usize>,
    data_offsets: [usize; 2],
}

pub struct Tensors {
    map: HashMap<String, (Vec<usize>, Vec<f32>)>,
}

impl Tensors {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() < 8 {
            return Err(Error::Model("safetensors 파일이 너무 작음".into()));
        }
        let hlen = u64::from_le_bytes(bytes[..8].try_into().unwrap()) as usize;
        let header: HashMap<String, serde_json::Value> =
            serde_json::from_slice(&bytes[8..8 + hlen])
                .map_err(|e| Error::Model(format!("헤더 읽기 실패: {e}")))?;
        let body = &bytes[8 + hlen..];
        let mut map = HashMap::new();
        for (name, v) in header {
            if name == "__metadata__" {
                continue;
            }
            let e: Entry = serde_json::from_value(v).map_err(|e| Error::Model(e.to_string()))?;
            if e.dtype != "F32" {
                return Err(Error::Model(format!(
                    "{name}: {} 형식은 아직 지원 안 함",
                    e.dtype
                )));
            }
            let raw = &body[e.data_offsets[0]..e.data_offsets[1]];
            let data = raw
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
                .collect();
            map.insert(name, (e.shape, data));
        }
        Ok(Self { map })
    }

    pub fn vec(&self, name: &str) -> Result<Vec<f32>, Error> {
        self.map
            .get(name)
            .map(|(_, d)| d.clone())
            .ok_or_else(|| Error::Model(format!("가중치 없음: {name}")))
    }

    pub fn mat(&self, name: &str) -> Result<Mat, Error> {
        let (shape, data) = self
            .map
            .get(name)
            .ok_or_else(|| Error::Model(format!("가중치 없음: {name}")))?;
        if shape.len() != 2 {
            return Err(Error::Model(format!("{name}: 2차원이 아님")));
        }
        Ok(Mat {
            rows: shape[0],
            cols: shape[1],
            data: data.clone(),
        })
    }
}

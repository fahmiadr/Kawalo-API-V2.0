//! Request and response shapes of `/me/home-layout`.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::core::error::ApiError;
use crate::core::extract::schema::{Body, Field, Kind, NO_LIMIT, Schema};
use crate::features::faces::unprocessable;

/// Card keys the app knows; the app owns their meaning.
const KEYS: [&str; 6] = ["ptt", "sos", "inc", "pat", "gst", "sch"];
const DEFAULT_ORDER: [&str; 4] = ["ptt", "sos", "inc", "pat"];
/// SOS is a safety feature: it must always be on the home screen.
const SOS: &str = "sos";
const MAX_VISIBLE: usize = 4;
const KEY: Kind = Kind::text(0, 16);

/// What is stored in `tkhomelayout.Layout`.
#[derive(Debug, Serialize, Deserialize)]
pub struct Layout {
    pub order: Vec<String>,
    pub hidden: Vec<String>,
}

impl Layout {
    pub fn default_layout() -> Self {
        Self {
            order: DEFAULT_ORDER.map(str::to_owned).to_vec(),
            hidden: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LayoutData {
    #[serde(flatten)]
    pub layout: Layout,
    pub is_default: bool,
}

#[derive(Debug, Deserialize)]
pub struct LayoutBody {
    order: Vec<String>,
    hidden: Vec<String>,
}

impl Body for LayoutBody {
    const SCHEMA: Schema = Schema::new(&[
        Field::required("order", Kind::list(&KEY, 0, NO_LIMIT)),
        Field::required("hidden", Kind::list(&KEY, 0, NO_LIMIT)),
    ]);
}

impl LayoutBody {
    /// Card rules (422). The handler runs this before auth, like a schema failure.
    pub fn validate(self) -> Result<Layout, ApiError> {
        if let Some(key) = self
            .order
            .iter()
            .chain(&self.hidden)
            .find(|k| !KEYS.contains(&k.as_str()))
        {
            return Err(unprocessable(format!("Kartu '{key}' tidak dikenal.")));
        }
        if has_duplicates(&self.order) {
            return Err(unprocessable(
                "Urutan kartu tidak boleh berisi kartu ganda.",
            ));
        }
        if has_duplicates(&self.hidden) {
            return Err(unprocessable(
                "Daftar kartu tersembunyi tidak boleh berisi kartu ganda.",
            ));
        }
        if self.hidden.iter().any(|k| !self.order.contains(k)) {
            return Err(unprocessable(
                "Kartu yang disembunyikan harus ada di urutan kartu.",
            ));
        }
        if !self.order.iter().any(|k| k == SOS) {
            return Err(unprocessable("Kartu SOS wajib ada di beranda."));
        }
        if self.hidden.iter().any(|k| k == SOS) {
            return Err(unprocessable("Kartu SOS tidak boleh disembunyikan."));
        }
        let visible = self.order.len() - self.hidden.len();
        if !(1..=MAX_VISIBLE).contains(&visible) {
            return Err(unprocessable(format!(
                "Jumlah kartu yang tampil harus 1 sampai {MAX_VISIBLE}."
            )));
        }
        Ok(Layout {
            order: self.order,
            hidden: self.hidden,
        })
    }
}

fn has_duplicates(keys: &[String]) -> bool {
    let mut seen = HashSet::new();
    !keys.iter().all(|k| seen.insert(k))
}

//! Body and result shared by the batch delete endpoints (`DELETE /guests`, `DELETE /information`).

use serde::{Deserialize, Serialize};

use crate::core::extract::schema::{Body, Field, Kind, NO_LIMIT, Schema};

#[derive(Debug, Deserialize)]
pub struct DeleteBatchBody {
    pub ids: Vec<String>,
}

impl Body for DeleteBatchBody {
    const SCHEMA: Schema = Schema::new(&[Field::required(
        "ids",
        Kind::list(&Kind::digits(), 1, NO_LIMIT),
    )]);
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct DeleteBatchResult {
    pub deleted_count: u64,
}

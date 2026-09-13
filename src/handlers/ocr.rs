use crate::{
    auth::Device,
    error::Result,
    models::{OcrResponse, SubmitLabelRequest},
    state::AppState,
};
use axum::{Json, extract::State};

/// Per hour, per device. Adding a product means photographing three sides of
/// a pack and reviewing what was read, so a genuine contributor is nowhere
/// near this. Anything above it is a script.
const CONTRIBUTIONS_PER_HOUR: u32 = 20;

pub async fn submit_label(
    State(state): State<AppState>,
    device: Device,
    Json(body): Json<SubmitLabelRequest>,
) -> Result<Json<OcrResponse>> {
    state.limit("ocr", &device.id, CONTRIBUTIONS_PER_HOUR, 3600).await?;
    let response = state.ocr_service.submit_label(&body, &device.id).await?;
    Ok(Json(response))
}

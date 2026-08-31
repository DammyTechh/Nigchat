//! `/v1/notifications` — tones, account preferences, per-conversation settings.

use axum::extract::{Path, State};
use axum::Json;
use nigchat_application::devices::{
    UpdateConversationNotifications, UpdateNotificationPreferences,
};
use nigchat_domain::ids::ConversationId;
use nigchat_domain::values::{PreviewMode, QuietHours, Vibration};
use uuid::Uuid;

use super::dto::*;
use crate::error::{ApiError, ApiResult};
use crate::extract::CurrentUser;
use crate::state::ApiState;

/// List available notification tones
///
/// The client bundles the audio; the server stores the identifier. Adding a
/// tone is therefore a data change, not an app release.
#[utoipa::path(
    get, path = "/v1/notifications/tones", tag = "notifications", security(("bearer" = [])),
    responses((status = 200, body = Vec<NotificationToneResponse>))
)]
pub async fn list_tones(
    State(state): State<ApiState>,
    _user: CurrentUser,
) -> ApiResult<Json<Vec<NotificationToneResponse>>> {
    let tones = state.devices.list_tones().await.map_err(ApiError)?;

    Ok(Json(
        tones
            .into_iter()
            .map(|tone| NotificationToneResponse {
                id: tone.id,
                display_name: tone.display_name,
                category: tone.category,
                asset_name: tone.asset_name,
                is_default: tone.is_default,
            })
            .collect(),
    ))
}

/// Get my notification preferences
#[utoipa::path(
    get, path = "/v1/notifications/preferences", tag = "notifications", security(("bearer" = [])),
    responses((status = 200, body = NotificationPreferencesResponse))
)]
pub async fn get_preferences(
    State(state): State<ApiState>,
    user: CurrentUser,
) -> ApiResult<Json<NotificationPreferencesResponse>> {
    let prefs = state
        .devices
        .notification_preferences(user.user_id)
        .await
        .map_err(ApiError)?;
    Ok(Json(to_response(prefs)))
}

/// Update my notification preferences
///
/// Quiet hours are given in local minutes past midnight plus an IANA timezone,
/// so a window such as 22:00–07:00 survives travel and daylight saving. Calls
/// may be allowed to ring through it; messages never are. Security alerts
/// ignore every setting here — an attacker who could silence them could take
/// an account unnoticed.
#[utoipa::path(
    patch, path = "/v1/notifications/preferences", tag = "notifications",
    security(("bearer" = [])), request_body = UpdateNotificationPreferencesRequest,
    responses(
        (status = 200, body = NotificationPreferencesResponse),
        (status = 400, description = "Unknown tone id or invalid quiet hours", body = crate::error::ErrorResponse),
    )
)]
pub async fn update_preferences(
    State(state): State<ApiState>,
    user: CurrentUser,
    Json(body): Json<UpdateNotificationPreferencesRequest>,
) -> ApiResult<Json<NotificationPreferencesResponse>> {
    let preview_mode = body
        .preview_mode
        .as_deref()
        .map(PreviewMode::parse)
        .transpose()
        .map_err(ApiError)?;

    let vibration = body
        .vibration
        .as_deref()
        .map(Vibration::parse)
        .transpose()
        .map_err(ApiError)?;

    let quiet_hours = match body.quiet_hours {
        None => None,
        Some(None) => Some(None),
        Some(Some(dto)) => Some(Some(
            QuietHours::new(
                dto.start_minute,
                dto.end_minute,
                dto.timezone,
                dto.allow_calls,
            )
            .map_err(ApiError)?,
        )),
    };

    let updated = state
        .devices
        .update_notification_preferences(
            user.user_id,
            UpdateNotificationPreferences {
                messages_enabled: body.messages_enabled,
                groups_enabled: body.groups_enabled,
                calls_enabled: body.calls_enabled,
                status_enabled: body.status_enabled,
                channels_enabled: body.channels_enabled,
                reactions_enabled: body.reactions_enabled,
                preview_mode,
                message_tone_id: body.message_tone_id,
                group_tone_id: body.group_tone_id,
                call_ringtone_id: body.call_ringtone_id,
                vibration,
                in_app_sounds: body.in_app_sounds,
                quiet_hours,
            },
        )
        .await
        .map_err(ApiError)?;

    Ok(Json(to_response(updated)))
}

/// Get per-conversation notification settings
#[utoipa::path(
    get, path = "/v1/conversations/{conversation_id}/notifications", tag = "notifications",
    security(("bearer" = [])), params(("conversation_id" = Uuid, Path,)),
    responses((status = 200, body = ConversationNotificationResponse))
)]
pub async fn get_conversation_settings(
    State(state): State<ApiState>,
    user: CurrentUser,
    Path(conversation_id): Path<Uuid>,
) -> ApiResult<Json<ConversationNotificationResponse>> {
    let settings = state
        .devices
        .conversation_notifications(ConversationId::from(conversation_id), user.user_id)
        .await
        .map_err(ApiError)?;

    Ok(Json(ConversationNotificationResponse {
        muted_until: settings.mute.muted_until,
        notify_on_mention: settings.notify_on_mention,
        tone_id: settings.tone_id,
        call_ringtone_id: settings.call_ringtone_id,
        vibration: settings.vibration.map(|v| v.as_str().to_string()),
        preview_mode: settings.preview_mode.map(|p| p.as_str().to_string()),
    }))
}

/// Set a custom tone for one conversation
///
/// Overrides the account default. Resolution order is per-conversation tone,
/// then account tone for the category, then the client's own default.
#[utoipa::path(
    patch, path = "/v1/conversations/{conversation_id}/notifications", tag = "notifications",
    security(("bearer" = [])), params(("conversation_id" = Uuid, Path,)),
    request_body = UpdateConversationNotificationsRequest,
    responses(
        (status = 200, body = ConversationNotificationResponse),
        (status = 400, description = "Unknown tone id", body = crate::error::ErrorResponse),
    )
)]
pub async fn update_conversation_settings(
    State(state): State<ApiState>,
    user: CurrentUser,
    Path(conversation_id): Path<Uuid>,
    Json(body): Json<UpdateConversationNotificationsRequest>,
) -> ApiResult<Json<ConversationNotificationResponse>> {
    let vibration = body
        .vibration
        .as_deref()
        .map(Vibration::parse)
        .transpose()
        .map_err(ApiError)?;

    let preview_mode = body
        .preview_mode
        .as_deref()
        .map(PreviewMode::parse)
        .transpose()
        .map_err(ApiError)?;

    let settings = state
        .devices
        .update_conversation_notifications(
            ConversationId::from(conversation_id),
            user.user_id,
            UpdateConversationNotifications {
                notify_on_mention: body.notify_on_mention,
                tone_id: body.tone_id.map(Some),
                call_ringtone_id: body.call_ringtone_id.map(Some),
                vibration: vibration.map(Some),
                preview_mode: preview_mode.map(Some),
            },
        )
        .await
        .map_err(ApiError)?;

    Ok(Json(ConversationNotificationResponse {
        muted_until: settings.mute.muted_until,
        notify_on_mention: settings.notify_on_mention,
        tone_id: settings.tone_id,
        call_ringtone_id: settings.call_ringtone_id,
        vibration: settings.vibration.map(|v| v.as_str().to_string()),
        preview_mode: settings.preview_mode.map(|p| p.as_str().to_string()),
    }))
}

fn to_response(
    prefs: nigchat_domain::entities::NotificationPreferences,
) -> NotificationPreferencesResponse {
    NotificationPreferencesResponse {
        messages_enabled: prefs.messages_enabled,
        groups_enabled: prefs.groups_enabled,
        calls_enabled: prefs.calls_enabled,
        status_enabled: prefs.status_enabled,
        channels_enabled: prefs.channels_enabled,
        reactions_enabled: prefs.reactions_enabled,
        security_alerts_enabled: prefs.security_alerts_enabled,
        preview_mode: prefs.preview_mode.as_str().to_string(),
        message_tone_id: prefs.message_tone_id,
        group_tone_id: prefs.group_tone_id,
        call_ringtone_id: prefs.call_ringtone_id,
        vibration: prefs.vibration.as_str().to_string(),
        in_app_sounds: prefs.in_app_sounds,
        quiet_hours: prefs.quiet_hours.map(|quiet| QuietHoursDto {
            start_minute: quiet.start_minute,
            end_minute: quiet.end_minute,
            timezone: quiet.timezone,
            allow_calls: quiet.allow_calls,
        }),
    }
}

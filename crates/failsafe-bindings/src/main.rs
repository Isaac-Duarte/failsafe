use failsafe_core::api::{
    AccountResponse, ApiError, AuthLoginRequest, AuthLogoutRequest, AuthRefreshRequest,
    AuthRegisterRequest, AuthResponse, DeviceInfo, DeviceListResponse, DevicePatchRequest,
    DeviceUpsertRequest, PairingCreateResponse, PairingRedeemRequest,
};
use failsafe_core::feature::FeatureId;
use failsafe_screen::{ScreenFramePayload, ScreenQualityPreset};
use specta::Types;
use specta_typescript::Typescript;

fn main() {
    let types = Types::default()
        .register::<AuthRegisterRequest>()
        .register::<AuthLoginRequest>()
        .register::<AuthResponse>()
        .register::<AuthRefreshRequest>()
        .register::<AuthLogoutRequest>()
        .register::<AccountResponse>()
        .register::<DeviceUpsertRequest>()
        .register::<DeviceInfo>()
        .register::<DeviceListResponse>()
        .register::<DevicePatchRequest>()
        .register::<PairingCreateResponse>()
        .register::<PairingRedeemRequest>()
        .register::<ApiError>()
        .register::<FeatureId>()
        .register::<ScreenQualityPreset>()
        .register::<ScreenFramePayload>();

    let output = concat!(env!("CARGO_MANIFEST_DIR"), "/../../failsafe-ui/src/lib/bindings.ts");
    Typescript::default()
        .export_to(output, &types, specta_serde::Format)
        .expect("failed to export TypeScript bindings");
}

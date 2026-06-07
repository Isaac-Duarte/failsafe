use failsafe_core::api::{
    AccountResponse, ApiError, AuthLoginRequest, AuthLogoutRequest, AuthMfaLoginRequest,
    AuthRefreshRequest, AuthRegisterRequest, AuthResponse, ChangePasswordRequest, DeviceInfo,
    DeviceListResponse, DevicePatchRequest, DeviceUpsertRequest, PairingCreateResponse,
    PairingRedeemRequest, TotpDisableRequest, TotpEnableRequest, TotpEnableResponse,
    TotpSetupResponse,
};
use failsafe_core::feature::FeatureId;
use failsafe_core::screen::ScreenInfo;
use specta::Types;
use specta_typescript::Typescript;

fn main() {
    let types = Types::default()
        .register::<AuthRegisterRequest>()
        .register::<AuthLoginRequest>()
        .register::<AuthMfaLoginRequest>()
        .register::<AuthResponse>()
        .register::<AuthRefreshRequest>()
        .register::<AuthLogoutRequest>()
        .register::<AccountResponse>()
        .register::<TotpSetupResponse>()
        .register::<TotpEnableRequest>()
        .register::<TotpEnableResponse>()
        .register::<TotpDisableRequest>()
        .register::<ChangePasswordRequest>()
        .register::<DeviceUpsertRequest>()
        .register::<DeviceInfo>()
        .register::<DeviceListResponse>()
        .register::<DevicePatchRequest>()
        .register::<PairingCreateResponse>()
        .register::<PairingRedeemRequest>()
        .register::<ApiError>()
        .register::<FeatureId>()
        .register::<ScreenInfo>();

    let output = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../failsafe-ui/src/lib/bindings.ts"
    );
    Typescript::default()
        .export_to(output, &types, specta_serde::Format)
        .expect("failed to export TypeScript bindings");
}

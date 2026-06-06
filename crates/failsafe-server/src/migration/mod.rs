pub use sea_orm_migration::prelude::*;

mod m20250605_000001_create_tables;
mod m20250605_000002_create_pairing_codes;
mod m20250605_000003_add_device_deleted_at;
mod m20250606_000004_create_refresh_tokens;
mod m20250606_000005_add_account_2fa;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20250605_000001_create_tables::Migration),
            Box::new(m20250605_000002_create_pairing_codes::Migration),
            Box::new(m20250605_000003_add_device_deleted_at::Migration),
            Box::new(m20250606_000004_create_refresh_tokens::Migration),
            Box::new(m20250606_000005_add_account_2fa::Migration),
        ]
    }
}

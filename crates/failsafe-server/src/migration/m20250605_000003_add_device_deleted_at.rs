use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Device::Table)
                    .add_column(
                        ColumnDef::new(Device::DeletedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Device::Table)
                    .drop_column(Device::DeletedAt)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Device {
    Table,
    DeletedAt,
}

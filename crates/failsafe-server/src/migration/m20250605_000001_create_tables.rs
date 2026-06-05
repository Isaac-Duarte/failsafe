use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Account::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Account::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Account::Email)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(Account::PasswordHash)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Account::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Device::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Device::DeviceId)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Device::AccountId).uuid().not_null())
                    .col(ColumnDef::new(Device::Name).string().not_null())
                    .col(
                        ColumnDef::new(Device::IrohPublicKey)
                            .string()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Device::EnabledFeatures).json().not_null())
                    .col(ColumnDef::new(Device::LastSeen).timestamp_with_time_zone())
                    .col(
                        ColumnDef::new(Device::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_device_account")
                            .from(Device::Table, Device::AccountId)
                            .to(Account::Table, Account::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_device_account_id")
                    .table(Device::Table)
                    .col(Device::AccountId)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Device::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Account::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Account {
    Table,
    Id,
    Email,
    PasswordHash,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Device {
    Table,
    DeviceId,
    AccountId,
    Name,
    IrohPublicKey,
    EnabledFeatures,
    LastSeen,
    CreatedAt,
}

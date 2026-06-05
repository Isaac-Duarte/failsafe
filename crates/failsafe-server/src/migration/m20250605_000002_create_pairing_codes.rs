use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(PairingCode::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(PairingCode::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(PairingCode::AccountId).uuid().not_null())
                    .col(
                        ColumnDef::new(PairingCode::Code)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(PairingCode::ExpiresAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(PairingCode::UsedAt)
                            .timestamp_with_time_zone(),
                    )
                    .col(
                        ColumnDef::new(PairingCode::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_pairing_code_account")
                            .from(PairingCode::Table, PairingCode::AccountId)
                            .to(Account::Table, Account::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(PairingCode::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum PairingCode {
    Table,
    Id,
    AccountId,
    Code,
    ExpiresAt,
    UsedAt,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Account {
    Table,
    Id,
}

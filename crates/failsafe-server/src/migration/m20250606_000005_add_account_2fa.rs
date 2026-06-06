use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Account::Table)
                    .add_column(ColumnDef::new(Account::TotpSecret).string())
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Account::Table)
                    .add_column(
                        ColumnDef::new(Account::TotpEnabled)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(RecoveryCode::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(RecoveryCode::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(RecoveryCode::AccountId).uuid().not_null())
                    .col(
                        ColumnDef::new(RecoveryCode::CodeHash)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(RecoveryCode::UsedAt).timestamp_with_time_zone())
                    .col(
                        ColumnDef::new(RecoveryCode::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_recovery_code_account")
                            .from(RecoveryCode::Table, RecoveryCode::AccountId)
                            .to(Account::Table, Account::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_recovery_code_account_id")
                    .table(RecoveryCode::Table)
                    .col(RecoveryCode::AccountId)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(RecoveryCode::Table).to_owned())
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Account::Table)
                    .drop_column(Account::TotpSecret)
                    .drop_column(Account::TotpEnabled)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Account {
    Table,
    TotpSecret,
    TotpEnabled,
    Id,
}

#[derive(DeriveIden)]
enum RecoveryCode {
    Table,
    Id,
    AccountId,
    CodeHash,
    UsedAt,
    CreatedAt,
}

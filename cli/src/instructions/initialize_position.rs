use std::ops::Deref;

use anchor_client::solana_client::rpc_config::RpcSendTransactionConfig;
use anchor_client::solana_sdk::compute_budget::ComputeBudgetInstruction;
use anchor_client::solana_sdk::instruction::Instruction;
use anchor_client::solana_sdk::signature::Keypair;
use anchor_client::{solana_sdk::pubkey::Pubkey, solana_sdk::signer::Signer, Program};

use anchor_lang::prelude::AccountMeta;
use anchor_lang::ToAccountMetas;
use anyhow::*;
use lb_clmm::accounts;
use lb_clmm::instruction;
use lb_clmm::utils::pda::derive_event_authority_pda;
use mpl_token_metadata::accounts::Metadata;
use spl_associated_token_account::get_associated_token_address;

#[derive(Debug)]
pub struct InitPositionParameters {
    pub lb_pair: Pubkey,
    pub lower_bin_id: i32,
    pub width: i32,
    pub nft_mint: Option<Pubkey>,
}

pub async fn initialize_position<C: Deref<Target = impl Signer> + Clone>(
    params: InitPositionParameters,
    program: &Program<C>,
    transaction_config: RpcSendTransactionConfig,
    compute_unit_price: Option<Instruction>,
) -> Result<Pubkey> {
    let InitPositionParameters {
        lb_pair,
        lower_bin_id,
        width,
        nft_mint,
    } = params;

    let position_keypair = Keypair::new();

    let (event_authority, _bump) = derive_event_authority_pda();

    let mut accounts = accounts::InitializePosition {
        lb_pair,
        payer: program.payer(),
        position: position_keypair.pubkey(),
        owner: program.payer(),
        rent: anchor_client::solana_sdk::sysvar::rent::ID,
        system_program: anchor_client::solana_sdk::system_program::ID,
        event_authority,
        program: lb_clmm::ID,
    }
    .to_account_metas(None);

    if let Some(nft_mint) = nft_mint {
        let nft_ata = get_associated_token_address(&program.payer(), &nft_mint);
        let (nft_metadata, _bump) = Metadata::find_pda(&nft_mint);

        accounts.push(AccountMeta::new_readonly(nft_ata, false));
        accounts.push(AccountMeta::new_readonly(nft_metadata, false));
    }

    let ix = instruction::InitializePosition {
        lower_bin_id,
        width,
    };

    let mut request_builder = program.request();
    let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_limit(1_400_000);

    if let Some(compute_unit_price) = compute_unit_price {
        request_builder = request_builder.instruction(compute_unit_price);
    }

    let signature = request_builder
        .instruction(compute_budget_ix)
        .accounts(accounts)
        .args(ix)
        .signer(position_keypair.insecure_clone())
        .send_with_spinner_and_config(transaction_config)
        .await;

    signature?;

    Ok(position_keypair.pubkey())
}

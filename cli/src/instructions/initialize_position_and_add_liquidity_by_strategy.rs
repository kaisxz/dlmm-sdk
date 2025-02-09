use std::ops::Deref;

use anchor_client::solana_client::rpc_config::RpcSendTransactionConfig;
use anchor_client::solana_sdk::compute_budget::ComputeBudgetInstruction;
use anchor_client::solana_sdk::instruction::Instruction;
use anchor_client::solana_sdk::pubkey;
use anchor_client::solana_sdk::signature::Keypair;
use anchor_client::{solana_sdk::pubkey::Pubkey, solana_sdk::signer::Signer, Program};
use anchor_lang::InstructionData;

use anchor_lang::prelude::AccountMeta;
use anchor_lang::ToAccountMetas;
use anchor_spl::token::spl_token;
use anyhow::*;
use lb_clmm::accounts;
use lb_clmm::instruction;
use lb_clmm::instructions::deposit::{LiquidityParameterByStrategy, StrategyParameters};
use lb_clmm::state::lb_pair::LbPair;
use lb_clmm::utils::pda::{derive_bin_array_bitmap_extension, derive_event_authority_pda};
use mpl_token_metadata::accounts::Metadata;
use spl_associated_token_account::get_associated_token_address;

use super::utils::{get_bin_arrays_for_pair, get_bin_arrays_for_position, get_or_create_ata};

#[derive(Debug)]
pub struct InitPositionAndAddLiquidityByStrategyParameters {
    pub lb_pair: Pubkey,
    pub amount_x: u64,
    pub amount_y: u64,
    pub active_id: i32,
    pub max_active_bin_slippage: i32,
    pub strategy_parameters: StrategyParameters,
    pub lower_bin_id: i32,
    pub width: i32,
    pub nft_mint: Option<Pubkey>,
}

//TODO: funktioniert noch nicht
pub async fn initialize_position_and_add_liquidity_by_strategy<C: Deref<Target = impl Signer> + Clone>(
    params: InitPositionAndAddLiquidityByStrategyParameters,
    program: &Program<C>,
    transaction_config: RpcSendTransactionConfig,
    compute_unit_price: Option<Instruction>,
) -> Result<Pubkey> {
    let InitPositionAndAddLiquidityByStrategyParameters {
        lb_pair,
        amount_x,
        amount_y,
        active_id,
        max_active_bin_slippage,
        strategy_parameters,
        lower_bin_id,
        width,
        nft_mint,
    } = params;

    let position_keypair = Keypair::new();

    let (event_authority, _bump) = derive_event_authority_pda();

    let lb_pair_state: LbPair = program.account(lb_pair).await?;

    //TODO: Jetzt problem hier!
    let [bin_array_lower, bin_array_upper] = get_bin_arrays_for_pair(lb_pair, lower_bin_id).await?;

    //TODO: Vermutlich liegt das hierran: Account not found
    let user_token_x = get_or_create_ata(
        program,
        transaction_config,
        lb_pair_state.token_x_mint,
        program.payer(),
        compute_unit_price.clone(),
    )
    .await?;

    let user_token_y = get_or_create_ata(
        program,
        transaction_config,
        lb_pair_state.token_y_mint,
        program.payer(),
        compute_unit_price.clone(),
    )
    .await?;
    //

    let (bin_array_bitmap_extension, _bump) = derive_bin_array_bitmap_extension(lb_pair);
    let bin_array_bitmap_extension = if program
        .rpc()
        .get_account(&bin_array_bitmap_extension)
        .await
        .is_err()
    {
        None
    } else {
        Some(bin_array_bitmap_extension)
    };

    let mut initialize_position_accounts = accounts::InitializePosition {
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

    let modify_liquidity_accounts = accounts::ModifyLiquidity {
        bin_array_lower,
        bin_array_upper,
        lb_pair,
        bin_array_bitmap_extension,
        position: position_keypair.pubkey(),
        reserve_x: lb_pair_state.reserve_x,
        reserve_y: lb_pair_state.reserve_y,
        token_x_mint: lb_pair_state.token_x_mint,
        token_y_mint: lb_pair_state.token_y_mint,
        sender: program.payer(),
        user_token_x,
        user_token_y,
        // TODO: token 2022
        token_x_program: anchor_spl::token::ID,
        token_y_program: anchor_spl::token::ID,
        event_authority,
        program: lb_clmm::ID,
    };

    if let Some(nft_mint) = nft_mint {
        let nft_ata = get_associated_token_address(&program.payer(), &nft_mint);
        let (nft_metadata, _bump) = Metadata::find_pda(&nft_mint);

        initialize_position_accounts.push(AccountMeta::new_readonly(nft_ata, false));
        initialize_position_accounts.push(AccountMeta::new_readonly(nft_metadata, false));
    }

    let initialize_position_ix = Instruction {
        program_id: lb_clmm::ID,
        accounts: initialize_position_accounts,
        data: instruction::InitializePosition {
            lower_bin_id,
            width,
        }
        .data(),
    };

    let add_liquidity_ix = Instruction {
        program_id: lb_clmm::ID,
        accounts: modify_liquidity_accounts.to_account_metas(None),
        data: instruction::AddLiquidityByStrategy {
            liquidity_parameter: LiquidityParameterByStrategy {
                amount_x,
                amount_y,
                active_id,
                max_active_bin_slippage,
                strategy_parameters,
            },
        }
        .data(),
    };

    let initialize_account_ix = anchor_spl::token::spl_token::instruction::initialize_account3(
        &spl_token::ID,
        &position_keypair.pubkey(),
        &program.payer(),
        &program.payer(),
    )?;

    let mut request_builder = program.request();

    let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_limit(1_400_000);

    request_builder = request_builder
        .instruction(compute_budget_ix)
        //.instruction(initialize_account_ix) //TODO: Funktioniert noch nicht
        .instruction(initialize_position_ix)
        .instruction(add_liquidity_ix);

    if let Some(compute_unit_price) = compute_unit_price {
        request_builder = request_builder.instruction(compute_unit_price);
    }

    let wsol_mint = pubkey!("So11111111111111111111111111111111111111112");
    if lb_pair_state.token_x_mint == wsol_mint || lb_pair_state.token_y_mint == wsol_mint {
        let wsol_account = get_associated_token_address(&program.payer(), &wsol_mint);

        if program.rpc().get_account(&wsol_account).await.is_ok() {
            let close_wsol_ix = spl_token::instruction::close_account(
                &spl_token::ID,
                &wsol_account,
                &program.payer(),
                &program.payer(),
                &[&program.payer()],
            )
            .unwrap();

            request_builder = request_builder.instruction(close_wsol_ix);
        }
    }

    let signature = request_builder
        .signer(position_keypair.insecure_clone())
        .send_with_spinner_and_config(transaction_config)
        .await;

    signature?;

    Ok(position_keypair.pubkey())
}

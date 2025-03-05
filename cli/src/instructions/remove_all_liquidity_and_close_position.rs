use std::ops::Deref;

use anchor_client::solana_client::rpc_config::RpcSendTransactionConfig;

use anchor_client::solana_sdk::compute_budget::ComputeBudgetInstruction;
use anchor_client::solana_sdk::instruction::Instruction;
use anchor_client::{solana_sdk::pubkey::Pubkey, solana_sdk::signer::Signer, Program};
use anchor_lang::solana_program::pubkey;
use anchor_lang::InstructionData;
use anchor_lang::ToAccountMetas;

use anchor_spl::token::spl_token;
use anyhow::*;
use lb_clmm::accounts;
use lb_clmm::instruction;
use lb_clmm::state::lb_pair::LbPair;
use lb_clmm::utils::pda::{derive_bin_array_bitmap_extension, derive_event_authority_pda};
use spl_associated_token_account::get_associated_token_address;

use crate::instructions::utils::{get_bin_arrays_for_position, get_or_create_ata};

pub struct RemoveAllLiquidityAndClosePositionParameters {
    pub lb_pair: Pubkey,
    pub position: Pubkey,
}

pub async fn remove_all_liquidity_and_close_position<C: Deref<Target = impl Signer> + Clone>(
    params: RemoveAllLiquidityAndClosePositionParameters,
    program: &Program<C>,
    transaction_config: RpcSendTransactionConfig,
    compute_unit_price: Option<Instruction>,
) -> Result<()> {
    let RemoveAllLiquidityAndClosePositionParameters { lb_pair, position } = params;

    let lb_pair_state: LbPair = program.account(lb_pair).await?;

    let [bin_array_lower, bin_array_upper] = get_bin_arrays_for_position(program, position).await?;

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

    // TODO: id and price slippage
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

    let (event_authority, _bump) = derive_event_authority_pda();

    let modify_liquidity_accounts = accounts::ModifyLiquidity {
        bin_array_lower,
        bin_array_upper,
        lb_pair,
        bin_array_bitmap_extension,
        position,
        reserve_x: lb_pair_state.reserve_x,
        reserve_y: lb_pair_state.reserve_y,
        token_x_mint: lb_pair_state.token_x_mint,
        token_y_mint: lb_pair_state.token_y_mint,
        sender: program.payer(),
        user_token_x,
        user_token_y,
        token_x_program: anchor_spl::token::ID,
        token_y_program: anchor_spl::token::ID,
        event_authority,
        program: lb_clmm::ID,
    };
    //TODO: Add ClaimFee and ClosePosition Account
    let claim_fee_accounts = accounts::ClaimFee {
        bin_array_lower,
        bin_array_upper,
        lb_pair,
        sender: program.payer(),
        position,
        reserve_x: lb_pair_state.reserve_x,
        reserve_y: lb_pair_state.reserve_y,
        token_program: anchor_spl::token::ID,
        token_x_mint: lb_pair_state.token_x_mint,
        token_y_mint: lb_pair_state.token_y_mint,
        user_token_x,
        user_token_y,
        event_authority,
        program: lb_clmm::ID,
    };
    let close_position_accounts = accounts::ClosePosition {
        bin_array_lower,
        bin_array_upper,
        lb_pair,
        sender: program.payer(),
        position,
        rent_receiver: program.payer(),
        event_authority,
        program: lb_clmm::ID,
    };

    //

    let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_limit(1_400_000);

    let remove_all_liquidity_ix = Instruction {
        program_id: lb_clmm::ID,
        accounts: modify_liquidity_accounts.to_account_metas(None),
        data: instruction::RemoveAllLiquidity {}.data(),
    };

    let claim_fee_ix = Instruction {
        program_id: lb_clmm::ID,
        accounts: claim_fee_accounts.to_account_metas(None),
        data: instruction::ClaimFee {}.data(),
    };

    let close_position_ix = Instruction {
        program_id: lb_clmm::ID,
        accounts: close_position_accounts.to_account_metas(None),
        data: instruction::ClosePosition {}.data(),
    };

    let mut request_builder = program.request();

    request_builder = request_builder
        .instruction(remove_all_liquidity_ix)
        .instruction(claim_fee_ix)
        .instruction(close_position_ix);

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

    request_builder = request_builder.instruction(compute_budget_ix);

    if let Some(compute_unit_price) = compute_unit_price {
        request_builder = request_builder.instruction(compute_unit_price);
    }

    request_builder
        .send_with_spinner_and_config(transaction_config)
        .await?;

    Ok(())
}

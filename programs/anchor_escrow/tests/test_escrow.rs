use {
    anchor_escrow::{Escrow, ESCROW_SEED},
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    anchor_spl::{associated_token, token},
    litesvm::{types::TransactionResult, LiteSVM},
    litesvm_token::{
        get_spl_account, spl_token::state::Account as TokenAccount, CreateAssociatedTokenAccount,
        CreateMint, MintTo,
    },
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
    spl_associated_token_account_interface::address::get_associated_token_address,
};

const MAKER_A: u64 = 1_000_000;
const TAKER_B: u64 = 1_000_000;
const DEPOSIT: u64 = 400_000;
const RECEIVE: u64 = 250_000;
const SEED: u64 = 42;

struct Env {
    svm: LiteSVM,
    pid: Pubkey,
    maker: Keypair,
    taker: Keypair,
    mint_a: Pubkey,
    mint_b: Pubkey,
}

fn setup() -> Env {
    let pid = anchor_escrow::id();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(
        env!("CARGO_TARGET_TMPDIR"),
        "/../deploy/anchor_escrow.so"
    ));
    svm.add_program(pid, bytes).unwrap();

    let authority = Keypair::new();
    let maker = Keypair::new();
    let taker = Keypair::new();
    for kp in [&authority, &maker, &taker] {
        svm.airdrop(&kp.pubkey(), 10_000_000_000).unwrap();
    }

    let mint_a = CreateMint::new(&mut svm, &authority).decimals(6).send().unwrap();
    let mint_b = CreateMint::new(&mut svm, &authority).decimals(6).send().unwrap();

    let maker_ata_a = CreateAssociatedTokenAccount::new(&mut svm, &authority, &mint_a)
        .owner(&maker.pubkey())
        .send()
        .unwrap();
    MintTo::new(&mut svm, &authority, &mint_a, &maker_ata_a, MAKER_A).send().unwrap();

    let taker_ata_b = CreateAssociatedTokenAccount::new(&mut svm, &authority, &mint_b)
        .owner(&taker.pubkey())
        .send()
        .unwrap();
    MintTo::new(&mut svm, &authority, &mint_b, &taker_ata_b, TAKER_B).send().unwrap();

    Env { svm, pid, maker, taker, mint_a, mint_b }
}

fn ata(owner: &Pubkey, mint: &Pubkey) -> Pubkey {
    get_associated_token_address(owner, mint)
}

fn escrow_pda(pid: &Pubkey, maker: &Pubkey, seed: u64) -> Pubkey {
    Pubkey::find_program_address(&[ESCROW_SEED, maker.as_ref(), &seed.to_le_bytes()], pid).0
}

fn balance(svm: &LiteSVM, token_account: &Pubkey) -> u64 {
    get_spl_account::<TokenAccount>(svm, token_account).unwrap().amount
}

fn read_escrow(svm: &LiteSVM, escrow: &Pubkey) -> Escrow {
    let acc = svm.get_account(escrow).expect("escrow account missing");
    Escrow::try_deserialize(&mut acc.data.as_slice()).unwrap()
}

fn try_send(svm: &mut LiteSVM, payer: &Keypair, ix: Instruction) -> TransactionResult {
    svm.expire_blockhash();
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[payer]).unwrap();
    svm.send_transaction(tx)
}

fn send(svm: &mut LiteSVM, payer: &Keypair, ix: Instruction) {
    try_send(svm, payer, ix).unwrap_or_else(|err| {
        panic!("tx failed: {err:?}\nlogs: {logs:#?}", logs = err.meta.logs);
    });
}

fn expect_err(res: TransactionResult, needle: &str) {
    let err = res.expect_err("transaction should have failed");
    assert!(
        err.meta.logs.iter().any(|l| l.contains(needle)),
        "expected `{needle}` in logs: {:#?}",
        err.meta.logs
    );
}

// ---------- instruction builders ----------

fn ix_make(e: &Env, maker: &Pubkey, seed: u64, deposit: u64, receive: u64) -> Instruction {
    let escrow = escrow_pda(&e.pid, maker, seed);
    Instruction::new_with_bytes(
        e.pid,
        &anchor_escrow::instruction::Make { seed, deposit, receive }.data(),
        anchor_escrow::accounts::Make {
            maker: *maker,
            mint_a: e.mint_a,
            mint_b: e.mint_b,
            maker_ata_a: ata(maker, &e.mint_a),
            escrow,
            vault: ata(&escrow, &e.mint_a),
            associated_token_program: associated_token::ID,
            token_program: token::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn ix_take(e: &Env, taker: &Pubkey, maker: &Pubkey, seed: u64) -> Instruction {
    let escrow = escrow_pda(&e.pid, maker, seed);
    Instruction::new_with_bytes(
        e.pid,
        &anchor_escrow::instruction::Take {}.data(),
        anchor_escrow::accounts::Take {
            taker: *taker,
            maker: *maker,
            mint_a: e.mint_a,
            mint_b: e.mint_b,
            taker_ata_a: ata(taker, &e.mint_a),
            taker_ata_b: ata(taker, &e.mint_b),
            maker_ata_b: ata(maker, &e.mint_b),
            escrow,
            vault: ata(&escrow, &e.mint_a),
            associated_token_program: associated_token::ID,
            token_program: token::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn ix_refund(e: &Env, signer: &Pubkey, escrow_maker: &Pubkey, seed: u64) -> Instruction {
    let escrow = escrow_pda(&e.pid, escrow_maker, seed);
    Instruction::new_with_bytes(
        e.pid,
        &anchor_escrow::instruction::Refund {}.data(),
        anchor_escrow::accounts::Refund {
            maker: *signer,
            mint_a: e.mint_a,
            maker_ata_a: ata(signer, &e.mint_a),
            escrow,
            vault: ata(&escrow, &e.mint_a),
            associated_token_program: associated_token::ID,
            token_program: token::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn ix_update(e: &Env, signer: &Pubkey, escrow_maker: &Pubkey, seed: u64, new_receive: u64) -> Instruction {
    Instruction::new_with_bytes(
        e.pid,
        &anchor_escrow::instruction::Update { new_receive }.data(),
        anchor_escrow::accounts::Update {
            maker: *signer,
            escrow: escrow_pda(&e.pid, escrow_maker, seed),
        }
        .to_account_metas(None),
    )
}

// ---------- tests ----------

#[test]
fn make_locks_tokens_in_vault() {
    let mut e = setup();
    let maker = e.maker.pubkey();
    let ix = ix_make(&e, &maker, SEED, DEPOSIT, RECEIVE);
    send(&mut e.svm, &e.maker, ix);

    let escrow = escrow_pda(&e.pid, &maker, SEED);
    let state = read_escrow(&e.svm, &escrow);
    assert_eq!(state.seed, SEED);
    assert_eq!(state.maker, maker);
    assert_eq!(state.mint_a, e.mint_a);
    assert_eq!(state.mint_b, e.mint_b);
    assert_eq!(state.receive, RECEIVE);

    assert_eq!(balance(&e.svm, &ata(&escrow, &e.mint_a)), DEPOSIT, "vault holds deposit");
    assert_eq!(balance(&e.svm, &ata(&maker, &e.mint_a)), MAKER_A - DEPOSIT);
}

#[test]
fn take_swaps_tokens_and_closes_accounts() {
    let mut e = setup();
    let (maker, taker) = (e.maker.pubkey(), e.taker.pubkey());
    let ix = ix_make(&e, &maker, SEED, DEPOSIT, RECEIVE);
    send(&mut e.svm, &e.maker, ix);

    let escrow = escrow_pda(&e.pid, &maker, SEED);
    let vault = ata(&escrow, &e.mint_a);
    let maker_lamports_before = e.svm.get_balance(&maker).unwrap();
    let rent_back = e.svm.get_balance(&escrow).unwrap() + e.svm.get_balance(&vault).unwrap();

    let ix = ix_take(&e, &taker, &maker, SEED);
    send(&mut e.svm, &e.taker, ix);

    // taker got mint A (ATA was created on the fly), maker got mint B
    assert_eq!(balance(&e.svm, &ata(&taker, &e.mint_a)), DEPOSIT);
    assert_eq!(balance(&e.svm, &ata(&taker, &e.mint_b)), TAKER_B - RECEIVE);
    assert_eq!(balance(&e.svm, &ata(&maker, &e.mint_b)), RECEIVE);

    // escrow + vault closed, rent returned to maker
    assert!(e.svm.get_account(&escrow).map_or(true, |a| a.lamports == 0));
    assert!(e.svm.get_account(&vault).map_or(true, |a| a.lamports == 0));
    assert_eq!(e.svm.get_balance(&maker).unwrap(), maker_lamports_before + rent_back);
}

#[test]
fn refund_returns_tokens_and_closes_accounts() {
    let mut e = setup();
    let maker = e.maker.pubkey();
    let ix = ix_make(&e, &maker, SEED, DEPOSIT, RECEIVE);
    send(&mut e.svm, &e.maker, ix);

    let ix = ix_refund(&e, &maker, &maker, SEED);
    send(&mut e.svm, &e.maker, ix);

    let escrow = escrow_pda(&e.pid, &maker, SEED);
    assert_eq!(balance(&e.svm, &ata(&maker, &e.mint_a)), MAKER_A, "maker made whole");
    assert!(e.svm.get_account(&escrow).map_or(true, |a| a.lamports == 0));
    assert!(e.svm.get_account(&ata(&escrow, &e.mint_a)).map_or(true, |a| a.lamports == 0));
}

#[test]
fn update_changes_price_and_take_uses_new_price() {
    let mut e = setup();
    let (maker, taker) = (e.maker.pubkey(), e.taker.pubkey());
    let ix = ix_make(&e, &maker, SEED, DEPOSIT, RECEIVE);
    send(&mut e.svm, &e.maker, ix);

    let new_receive = 600_000;
    let ix = ix_update(&e, &maker, &maker, SEED, new_receive);
    send(&mut e.svm, &e.maker, ix);
    let escrow = escrow_pda(&e.pid, &maker, SEED);
    assert_eq!(read_escrow(&e.svm, &escrow).receive, new_receive);

    let ix = ix_take(&e, &taker, &maker, SEED);
    send(&mut e.svm, &e.taker, ix);
    assert_eq!(balance(&e.svm, &ata(&maker, &e.mint_b)), new_receive);
    assert_eq!(balance(&e.svm, &ata(&taker, &e.mint_b)), TAKER_B - new_receive);
}

#[test]
fn update_to_zero_fails() {
    let mut e = setup();
    let maker = e.maker.pubkey();
    let ix = ix_make(&e, &maker, SEED, DEPOSIT, RECEIVE);
    send(&mut e.svm, &e.maker, ix);
    let ix = ix_update(&e, &maker, &maker, SEED, 0);
    expect_err(try_send(&mut e.svm, &e.maker, ix), "InvalidAmount");
}

#[test]
fn only_maker_can_update() {
    let mut e = setup();
    let (maker, taker) = (e.maker.pubkey(), e.taker.pubkey());
    let ix = ix_make(&e, &maker, SEED, DEPOSIT, RECEIVE);
    send(&mut e.svm, &e.maker, ix);

    // Taker signs and points at the maker's escrow: seeds/has_one must reject it.
    let ix = ix_update(&e, &taker, &maker, SEED, 1);
    assert!(try_send(&mut e.svm, &e.taker, ix).is_err());
    let escrow = escrow_pda(&e.pid, &maker, SEED);
    assert_eq!(read_escrow(&e.svm, &escrow).receive, RECEIVE, "price unchanged");
}

#[test]
fn only_maker_can_refund() {
    let mut e = setup();
    let (maker, taker) = (e.maker.pubkey(), e.taker.pubkey());
    let ix = ix_make(&e, &maker, SEED, DEPOSIT, RECEIVE);
    send(&mut e.svm, &e.maker, ix);

    let ix = ix_refund(&e, &taker, &maker, SEED);
    assert!(try_send(&mut e.svm, &e.taker, ix).is_err());
    let escrow = escrow_pda(&e.pid, &maker, SEED);
    assert_eq!(balance(&e.svm, &ata(&escrow, &e.mint_a)), DEPOSIT, "vault untouched");
}

#[test]
fn take_fails_when_taker_cannot_pay() {
    let mut e = setup();
    let (maker, taker) = (e.maker.pubkey(), e.taker.pubkey());
    let ix = ix_make(&e, &maker, SEED, DEPOSIT, TAKER_B + 1);
    send(&mut e.svm, &e.maker, ix);

    let ix = ix_take(&e, &taker, &maker, SEED);
    expect_err(try_send(&mut e.svm, &e.taker, ix), "insufficient funds");
    let escrow = escrow_pda(&e.pid, &maker, SEED);
    assert_eq!(balance(&e.svm, &ata(&escrow, &e.mint_a)), DEPOSIT, "vault untouched");
}

#[test]
fn cannot_take_after_refund() {
    let mut e = setup();
    let (maker, taker) = (e.maker.pubkey(), e.taker.pubkey());
    let ix = ix_make(&e, &maker, SEED, DEPOSIT, RECEIVE);
    send(&mut e.svm, &e.maker, ix);
    let ix = ix_refund(&e, &maker, &maker, SEED);
    send(&mut e.svm, &e.maker, ix);

    let ix = ix_take(&e, &taker, &maker, SEED);
    assert!(try_send(&mut e.svm, &e.taker, ix).is_err());
}

#[test]
fn make_rejects_zero_amounts() {
    let mut e = setup();
    let maker = e.maker.pubkey();
    let ix = ix_make(&e, &maker, SEED, 0, RECEIVE);
    expect_err(try_send(&mut e.svm, &e.maker, ix), "InvalidAmount");
    let ix = ix_make(&e, &maker, SEED, DEPOSIT, 0);
    expect_err(try_send(&mut e.svm, &e.maker, ix), "InvalidAmount");
}

#[test]
fn maker_can_open_multiple_escrows_with_different_seeds() {
    let mut e = setup();
    let maker = e.maker.pubkey();
    let ix = ix_make(&e, &maker, 1, 100_000, RECEIVE);
    send(&mut e.svm, &e.maker, ix);
    let ix = ix_make(&e, &maker, 2, 200_000, RECEIVE);
    send(&mut e.svm, &e.maker, ix);

    let v1 = ata(&escrow_pda(&e.pid, &maker, 1), &e.mint_a);
    let v2 = ata(&escrow_pda(&e.pid, &maker, 2), &e.mint_a);
    assert_eq!(balance(&e.svm, &v1), 100_000);
    assert_eq!(balance(&e.svm, &v2), 200_000);

    // Same seed twice must fail (escrow PDA already exists).
    let ix = ix_make(&e, &maker, 1, 100_000, RECEIVE);
    assert!(try_send(&mut e.svm, &e.maker, ix).is_err());
}

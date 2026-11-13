export type MilestoneStatus = "Pending" | "Submitted" | "Challenged" | "Accepted" | "Rejected" | "Failed";
export interface Milestone {
    title: string;
    weight_bps: number;
    deadline_ms: number;
    status: MilestoneStatus;
    proof_uri: string | null;
    /** sha256 of the file at proof_uri, 64 lowercase hex characters. */
    proof_hash: string | null;
    submitted_at_ms: number | null;
    challenger: string | null;
    challenge_reason: string | null;
    challenged_at_ms: number | null;
    /** Bonded challenge requests ever made on this milestone. Absent on goals without a treasury build. */
    challenge_attempts?: number;
    /** The latest bonded challenge request (treasury mode); a new request replaces it. */
    case?: ChallengeCase | null;
}
/** Stage of a bonded challenge request (SPEC 6a). */
export type CaseStage = "RegisterPending" | "RegisterRetryable" | "Ready" | "VerifyPending" | "BindPending" | "BindRetryable" | "Bound" | "Qualified" | "DecisionPending" | "DecisionRetryable" | "SettlementPending" | "SettlementRetryable" | "Settled" | "TimeoutRequested" | "Orphaned";
/** Stages after which the goal does nothing more with the bond. */
export declare const FINAL_CASE_STAGES: readonly CaseStage[];
/** Verifier decision attested to the Treasury. */
export type CaseDecision = "QualificationRejected" | "ProofValid" | "ProofInvalidFinal";
/** `milestones[i].case` in `get_goal` (treasury mode). Amounts: yoctoNEAR strings; times: ns. */
export interface ChallengeCase {
    proof_id: string;
    case_id: string;
    /** The challenger who filed the request and deposits the bond. */
    payer: string;
    reason: string;
    /** F: the milestone's share of counted fees at the request. */
    fee_snapshot: string;
    /** B: the bond to deposit in the Treasury. */
    amount: string;
    reward_cap: string;
    deposit_until_ns: number;
    bind_until_ns: number;
    /** Verifier deadline; after it the bond is refunded neutrally. */
    refund_after_ns: number;
    /** Treasury escrow id; null until register_intent succeeds. */
    escrow_id: string | null;
    stage: CaseStage;
    decision: CaseDecision | null;
    nonce: number;
}
/** `get_goal` response. */
export interface GoalView {
    creator: string;
    verifier: string;
    token: string | null;
    goal_title: string;
    /** Link to the goal spec file, pinned to a commit. */
    goal_uri: string;
    /** sha256 of the file at goal_uri, 64 lowercase hex characters. */
    goal_spec_hash: string;
    immediate_bps: number;
    challenge_window_ms: number;
    milestones: Milestone[];
    /** Bond Treasury account; null or absent: free challenges (SPEC 1.0). */
    treasury?: string | null;
}
/** `get_balances` response. All amounts are yoctoNEAR strings. */
export interface BalancesView {
    total_received: string;
    immediate: string;
    locked: string;
    unlocked: string;
    forfeited: string;
    pending: string;
    claimed: string;
    claimable: string;
    /** Received by the account but not yet counted by `sync`. */
    unsynced: string;
}
/** Treasury bond status (`get_bond(escrow_id).status`); null while only the intent exists. */
export type BondStatus = "Deposited" | "Bound" | "RefundPending" | "Refunded" | "Slashed" | "RefundRetryable";
/** Treasury `get_bond` response (proofee-treasury/0.2.0). */
export interface TreasuryBond {
    escrow_id: string;
    goal: string;
    proof_id: string;
    case_id: string;
    submission_id: string;
    payer: string;
    amount: string;
    deposit_until_ns: number;
    bind_until_ns: number;
    refund_after_ns: number;
    created_at_ns: number | null;
    status: BondStatus | null;
    settlement_id: string | null;
    outcome: "REFUND" | "SLASH" | null;
    attempt: number;
    reward_cap: string;
    decision: CaseDecision | "NeutralRefund" | null;
    demo: boolean;
}
export declare const TESTNET_RPC_URL = "https://rpc.testnet.near.org";
export declare const DEMO_GOAL_ID = "goal-4.proofee-anton.testnet";
export interface ReadOptions {
    /** JSON-RPC endpoint. Default: TESTNET_RPC_URL. */
    rpcUrl?: string;
    /** Custom fetch (tests, server runtimes). Default: global fetch. */
    fetch?: typeof fetch;
}
/** Calls a view method and parses its JSON result. */
export declare function viewFunction<T>(contractId: string, methodName: string, args?: object, options?: ReadOptions): Promise<T>;
export declare function getGoal(contractId: string, options?: ReadOptions): Promise<GoalView>;
export declare function getBalances(contractId: string, options?: ReadOptions): Promise<BalancesView>;
/** Treasury bond of a challenge request; null if the Treasury does not know the escrow id. */
export declare function getBond(treasuryId: string, escrowId: string, options?: ReadOptions): Promise<TreasuryBond | null>;
/** Account page on testnet NearBlocks. */
export declare function nearblocksUrl(accountId: string): string;
/**
 * Contract time in nanoseconds → milliseconds (for `new Date`). ns values (~1.8e18) exceed
 * Number.MAX_SAFE_INTEGER, so JSON.parse rounds them by up to a few hundred ns; rounding to the
 * nearest ms hides that. Fine for display; do not compare ns values for equality.
 */
export declare function nsToMs(ns: number): number;
/** Exact yoctoNEAR → NEAR as a decimal string: "700000000000000000000000" → "0.7". */
export declare function yoctoToNear(yocto: string): string;
export type BalancesInNear = {
    [K in keyof BalancesView]: string;
};
/** All `get_balances` fields converted to NEAR strings. */
export declare function balancesToNear(balances: BalancesView): BalancesInNear;
/** Share of all fees paid immediately, in percent: immediate_bps / 100. */
export declare function immediateSharePct(goal: GoalView): number;
/**
 * Share of all fees that a milestone unlocks, in percent:
 * weight_bps × (10000 − immediate_bps) / 1e6. Example: 1875 bps at 2000 immediate → 15.
 */
export declare function milestoneSharePct(milestone: Milestone, immediateBps: number): number;
export interface SpecCheck {
    /** true if sha256 of the file at goal_uri equals goal_spec_hash. */
    ok: boolean;
    uri: string;
    expected: string;
    /** sha256 of the downloaded file; null if it could not be downloaded. */
    actual: string | null;
    /** The downloaded file as text, to show the spec in the UI. */
    content: string | null;
    error?: string;
}
/** sha256 in lowercase hex via crypto.subtle (needs https or localhost in browsers). */
export declare function sha256Hex(data: Uint8Array): Promise<string>;
/** Downloads goal_uri and compares its sha256 with goal_spec_hash. Never throws. */
export declare function verifySpec(goal: GoalView, options?: {
    fetch?: typeof fetch;
}): Promise<SpecCheck>;
/** 100 TGas. */
export declare const DEFAULT_GAS = "100000000000000";
export declare const ZERO_DEPOSIT = "0";
export interface FunctionCallAction {
    type: "FunctionCall";
    params: {
        methodName: string;
        args: object;
        gas: string;
        deposit: string;
    };
}
/** Pass to `wallet.signAndSendTransaction(tx)` (wallet selector, near-connect). */
export interface Transaction {
    receiverId: string;
    actions: FunctionCallAction[];
}
/** Anyone: count NEAR received since the last sync as income. */
export declare const syncTx: (goalId: string) => Transaction;
/** Creator: proof for milestone `index` (Pending → Submitted). proofHash: sha256, 64 lowercase hex. */
export declare const submitProofTx: (goalId: string, index: number, proofUri: string, proofHash: string) => Transaction;
/** Anyone except the creator and the verifier, while the window is open (Submitted → Challenged). */
export declare const challengeTx: (goalId: string, index: number, reason: string) => Transaction;
/** Anyone, after the challenge window (Submitted → Accepted). */
export declare const finalizeTx: (goalId: string, index: number) => Transaction;
/** Verifier, within 24 h of the challenge (Challenged → Accepted / Rejected). */
export declare const resolveTx: (goalId: string, index: number, accept: boolean) => Transaction;
/** Anyone, 24 h after the challenge if the verifier did not resolve (Challenged → Accepted). */
export declare const resolveTimeoutTx: (goalId: string, index: number) => Transaction;
/** Anyone, after the deadline of a milestone without a proof (Pending → Failed). */
export declare const expireTx: (goalId: string, index: number) => Transaction;
/** Creator: pay out everything claimable. */
export declare const claimTx: (goalId: string) => Transaction;

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

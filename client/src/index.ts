// ProoFee goal client: read a goal over NEAR JSON-RPC and build unsigned transactions.
// No dependencies: fetch, TextEncoder/TextDecoder and crypto.subtle only.
// Types follow the contract ABI (`cargo near abi` in contracts/goal); spec: docs/SPEC.md.

// ---- Types (contract ABI) ----

export type MilestoneStatus =
  | "Pending"
  | "Submitted"
  | "Challenged"
  | "Accepted"
  | "Rejected"
  | "Failed";

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
export type CaseStage =
  | "RegisterPending"
  | "RegisterRetryable"
  | "Ready"
  | "VerifyPending"
  | "BindPending"
  | "BindRetryable"
  | "Bound"
  | "Qualified"
  | "DecisionPending"
  | "DecisionRetryable"
  | "SettlementPending"
  | "SettlementRetryable"
  | "Settled"
  | "TimeoutRequested"
  | "Orphaned";

/** Stages after which the goal does nothing more with the bond. */
export const FINAL_CASE_STAGES: readonly CaseStage[] = ["Settled", "TimeoutRequested", "Orphaned"];

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
export type BondStatus =
  | "Deposited"
  | "Bound"
  | "RefundPending"
  | "Refunded"
  | "Slashed"
  | "RefundRetryable";

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

// ---- Reading ----

export const TESTNET_RPC_URL = "https://rpc.testnet.near.org";
export const DEMO_GOAL_ID = "goal-4.proofee-anton.testnet";

export interface ReadOptions {
  /** JSON-RPC endpoint. Default: TESTNET_RPC_URL. */
  rpcUrl?: string;
  /** Custom fetch (tests, server runtimes). Default: global fetch. */
  fetch?: typeof fetch;
}

/** Calls a view method and parses its JSON result. */
export async function viewFunction<T>(
  contractId: string,
  methodName: string,
  args: object = {},
  options: ReadOptions = {},
): Promise<T> {
  const doFetch = options.fetch ?? fetch;
  const response = await doFetch(options.rpcUrl ?? TESTNET_RPC_URL, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: "proofee",
      method: "query",
      params: {
        request_type: "call_function",
        finality: "final",
        account_id: contractId,
        method_name: methodName,
        args_base64: bytesToBase64(new TextEncoder().encode(JSON.stringify(args))),
      },
    }),
  });
  if (!response.ok) {
    throw new Error(`RPC HTTP ${response.status} for ${contractId}.${methodName}`);
  }
  const body = await response.json();
  // RPC errors come either at the top level or inside result (contract panics).
  const error = body.error ?? body.result?.error;
  if (error) {
    const text = typeof error === "string" ? error : JSON.stringify(error);
    throw new Error(`${contractId}.${methodName}: ${text}`);
  }
  const bytes = Uint8Array.from(body.result.result as number[]);
  return JSON.parse(new TextDecoder().decode(bytes)) as T;
}

export function getGoal(contractId: string, options?: ReadOptions): Promise<GoalView> {
  return viewFunction<GoalView>(contractId, "get_goal", {}, options);
}

export function getBalances(contractId: string, options?: ReadOptions): Promise<BalancesView> {
  return viewFunction<BalancesView>(contractId, "get_balances", {}, options);
}

/** Treasury bond of a challenge request; null if the Treasury does not know the escrow id. */
export function getBond(
  treasuryId: string,
  escrowId: string,
  options?: ReadOptions,
): Promise<TreasuryBond | null> {
  return viewFunction<TreasuryBond | null>(treasuryId, "get_bond", { escrow_id: escrowId }, options);
}

// ---- Formatting ----

/** Account page on testnet NearBlocks. */
export function nearblocksUrl(accountId: string): string {
  return `https://testnet.nearblocks.io/address/${encodeURIComponent(accountId)}`;
}

/**
 * Contract time in nanoseconds → milliseconds (for `new Date`). ns values (~1.8e18) exceed
 * Number.MAX_SAFE_INTEGER, so JSON.parse rounds them by up to a few hundred ns; rounding to the
 * nearest ms hides that. Fine for display; do not compare ns values for equality.
 */
export function nsToMs(ns: number): number {
  return Math.round(ns / 1_000_000);
}

const YOCTO_DECIMALS = 24;

/** Exact yoctoNEAR → NEAR as a decimal string: "700000000000000000000000" → "0.7". */
export function yoctoToNear(yocto: string): string {
  if (!/^\d+$/.test(yocto)) throw new Error(`Not a yoctoNEAR amount: ${yocto}`);
  const padded = yocto.replace(/^0+/, "").padStart(YOCTO_DECIMALS + 1, "0");
  const whole = padded.slice(0, -YOCTO_DECIMALS);
  const fraction = padded.slice(-YOCTO_DECIMALS).replace(/0+$/, "");
  return fraction ? `${whole}.${fraction}` : whole;
}

export type BalancesInNear = { [K in keyof BalancesView]: string };

/** All `get_balances` fields converted to NEAR strings. */
export function balancesToNear(balances: BalancesView): BalancesInNear {
  const result = {} as BalancesInNear;
  for (const key of Object.keys(balances) as (keyof BalancesView)[]) {
    result[key] = yoctoToNear(balances[key]);
  }
  return result;
}

/** Share of all fees paid immediately, in percent: immediate_bps / 100. */
export function immediateSharePct(goal: GoalView): number {
  return goal.immediate_bps / 100;
}

/**
 * Share of all fees that a milestone unlocks, in percent:
 * weight_bps × (10000 − immediate_bps) / 1e6. Example: 1875 bps at 2000 immediate → 15.
 */
export function milestoneSharePct(milestone: Milestone, immediateBps: number): number {
  return (milestone.weight_bps * (10000 - immediateBps)) / 1e6;
}

// ---- Goal spec check ----

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
export async function sha256Hex(data: Uint8Array): Promise<string> {
  if (!globalThis.crypto?.subtle) {
    throw new Error("crypto.subtle is unavailable (the page must be served over https or localhost)");
  }
  const digest = await globalThis.crypto.subtle.digest("SHA-256", data as BufferSource);
  return Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, "0")).join("");
}

/** Downloads goal_uri and compares its sha256 with goal_spec_hash. Never throws. */
export async function verifySpec(
  goal: GoalView,
  options: { fetch?: typeof fetch } = {},
): Promise<SpecCheck> {
  const doFetch = options.fetch ?? fetch;
  const base = { uri: goal.goal_uri, expected: goal.goal_spec_hash };
  try {
    const response = await doFetch(goal.goal_uri);
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const bytes = new Uint8Array(await response.arrayBuffer());
    const actual = await sha256Hex(bytes);
    return {
      ...base,
      ok: actual === goal.goal_spec_hash,
      actual,
      content: new TextDecoder().decode(bytes),
    };
  } catch (e) {
    return { ...base, ok: false, actual: null, content: null, error: String(e) };
  }
}

// ---- Transactions (unsigned, wallet selector / near-connect format) ----

/** 100 TGas. */
export const DEFAULT_GAS = "100000000000000";
export const ZERO_DEPOSIT = "0";

export interface FunctionCallAction {
  type: "FunctionCall";
  params: { methodName: string; args: object; gas: string; deposit: string };
}

/** Pass to `wallet.signAndSendTransaction(tx)` (wallet selector, near-connect). */
export interface Transaction {
  receiverId: string;
  actions: FunctionCallAction[];
}

function call(goalId: string, methodName: string, args: object = {}): Transaction {
  return {
    receiverId: goalId,
    actions: [
      { type: "FunctionCall", params: { methodName, args, gas: DEFAULT_GAS, deposit: ZERO_DEPOSIT } },
    ],
  };
}

/** Anyone: count NEAR received since the last sync as income. */
export const syncTx = (goalId: string) => call(goalId, "sync");

/** Creator: proof for milestone `index` (Pending → Submitted). proofHash: sha256, 64 lowercase hex. */
export const submitProofTx = (goalId: string, index: number, proofUri: string, proofHash: string) =>
  call(goalId, "submit_proof", { index, proof_uri: proofUri, proof_hash: proofHash });

/** Anyone except the creator and the verifier, while the window is open (Submitted → Challenged). */
export const challengeTx = (goalId: string, index: number, reason: string) =>
  call(goalId, "challenge", { index, reason });

/** Anyone, after the challenge window (Submitted → Accepted). */
export const finalizeTx = (goalId: string, index: number) => call(goalId, "finalize", { index });

/** Verifier, within 24 h of the challenge (Challenged → Accepted / Rejected). */
export const resolveTx = (goalId: string, index: number, accept: boolean) =>
  call(goalId, "resolve", { index, accept });

/** Anyone, 24 h after the challenge if the verifier did not resolve (Challenged → Accepted). */
export const resolveTimeoutTx = (goalId: string, index: number) =>
  call(goalId, "resolve_timeout", { index });

/** Anyone, after the deadline of a milestone without a proof (Pending → Failed). */
export const expireTx = (goalId: string, index: number) => call(goalId, "expire", { index });

/** Creator: pay out everything claimable. */
export const claimTx = (goalId: string) => call(goalId, "claim");

// ---- Helpers ----

function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary);
}

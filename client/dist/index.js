// ProoFee goal client: read a goal over NEAR JSON-RPC and build unsigned transactions.
// No dependencies: fetch, TextEncoder/TextDecoder and crypto.subtle only.
// Types follow the contract ABI (`cargo near abi` in contracts/goal); spec: docs/SPEC.md.
/** Stages after which the goal does nothing more with the bond. */
export const FINAL_CASE_STAGES = ["Settled", "TimeoutRequested", "Orphaned"];
// ---- Reading ----
export const TESTNET_RPC_URL = "https://rpc.testnet.near.org";
/** Demo goal with a Bond Treasury (docs/DEMO_RUN_TREASURY.md). */
export const DEMO_GOAL_ID = "goal-5.proofee-anton.testnet";
/** Earlier demo goal with free challenges, no treasury (docs/DEMO_RUN.md). */
export const FREE_CHALLENGE_GOAL_ID = "goal-4.proofee-anton.testnet";
/** Calls a view method and parses its JSON result. */
export async function viewFunction(contractId, methodName, args = {}, options = {}) {
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
    const bytes = Uint8Array.from(body.result.result);
    return JSON.parse(new TextDecoder().decode(bytes));
}
export function getGoal(contractId, options) {
    return viewFunction(contractId, "get_goal", {}, options);
}
export function getBalances(contractId, options) {
    return viewFunction(contractId, "get_balances", {}, options);
}
/** Treasury bond of a challenge request; null if the Treasury does not know the escrow id. */
export function getBond(treasuryId, escrowId, options) {
    return viewFunction(treasuryId, "get_bond", { escrow_id: escrowId }, options);
}
// ---- Formatting ----
/** Account page on testnet NearBlocks. */
export function nearblocksUrl(accountId) {
    return `https://testnet.nearblocks.io/address/${encodeURIComponent(accountId)}`;
}
/**
 * Contract time in nanoseconds → milliseconds (for `new Date`). ns values (~1.8e18) exceed
 * Number.MAX_SAFE_INTEGER, so JSON.parse rounds them by up to a few hundred ns; rounding to the
 * nearest ms hides that. Fine for display; do not compare ns values for equality.
 */
export function nsToMs(ns) {
    return Math.round(ns / 1000000);
}
const YOCTO_DECIMALS = 24;
/** Exact yoctoNEAR → NEAR as a decimal string: "700000000000000000000000" → "0.7". */
export function yoctoToNear(yocto) {
    if (!/^\d+$/.test(yocto))
        throw new Error(`Not a yoctoNEAR amount: ${yocto}`);
    const padded = yocto.replace(/^0+/, "").padStart(YOCTO_DECIMALS + 1, "0");
    const whole = padded.slice(0, -YOCTO_DECIMALS);
    const fraction = padded.slice(-YOCTO_DECIMALS).replace(/0+$/, "");
    return fraction ? `${whole}.${fraction}` : whole;
}
/** All `get_balances` fields converted to NEAR strings. */
export function balancesToNear(balances) {
    const result = {};
    for (const key of Object.keys(balances)) {
        result[key] = yoctoToNear(balances[key]);
    }
    return result;
}
/** Share of all fees paid immediately, in percent: immediate_bps / 100. */
export function immediateSharePct(goal) {
    return goal.immediate_bps / 100;
}
/**
 * Share of all fees that a milestone unlocks, in percent:
 * weight_bps × (10000 − immediate_bps) / 1e6. Example: 1875 bps at 2000 immediate → 15.
 */
export function milestoneSharePct(milestone, immediateBps) {
    return (milestone.weight_bps * (10000 - immediateBps)) / 1e6;
}
/** sha256 in lowercase hex via crypto.subtle (needs https or localhost in browsers). */
export async function sha256Hex(data) {
    if (!globalThis.crypto?.subtle) {
        throw new Error("crypto.subtle is unavailable (the page must be served over https or localhost)");
    }
    const digest = await globalThis.crypto.subtle.digest("SHA-256", data);
    return Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, "0")).join("");
}
/** Downloads goal_uri and compares its sha256 with goal_spec_hash. Never throws. */
export async function verifySpec(goal, options = {}) {
    const doFetch = options.fetch ?? fetch;
    const base = { uri: goal.goal_uri, expected: goal.goal_spec_hash };
    try {
        const response = await doFetch(goal.goal_uri);
        if (!response.ok)
            throw new Error(`HTTP ${response.status}`);
        const bytes = new Uint8Array(await response.arrayBuffer());
        const actual = await sha256Hex(bytes);
        return {
            ...base,
            ok: actual === goal.goal_spec_hash,
            actual,
            content: new TextDecoder().decode(bytes),
        };
    }
    catch (e) {
        return { ...base, ok: false, actual: null, content: null, error: String(e) };
    }
}
// ---- Transactions (unsigned, wallet selector / near-connect format) ----
/** 100 TGas. */
export const DEFAULT_GAS = "100000000000000";
export const ZERO_DEPOSIT = "0";
function call(goalId, methodName, args = {}) {
    return {
        receiverId: goalId,
        actions: [
            { type: "FunctionCall", params: { methodName, args, gas: DEFAULT_GAS, deposit: ZERO_DEPOSIT } },
        ],
    };
}
/** Anyone: count NEAR received since the last sync as income. */
export const syncTx = (goalId) => call(goalId, "sync");
/** Creator: proof for milestone `index` (Pending → Submitted). proofHash: sha256, 64 lowercase hex. */
export const submitProofTx = (goalId, index, proofUri, proofHash) => call(goalId, "submit_proof", { index, proof_uri: proofUri, proof_hash: proofHash });
/** Anyone except the creator and the verifier, while the window is open (Submitted → Challenged). */
export const challengeTx = (goalId, index, reason) => call(goalId, "challenge", { index, reason });
/** Anyone, after the challenge window (Submitted → Accepted). */
export const finalizeTx = (goalId, index) => call(goalId, "finalize", { index });
/** Verifier, within 24 h of the challenge (Challenged → Accepted / Rejected). */
export const resolveTx = (goalId, index, accept) => call(goalId, "resolve", { index, accept });
/** Anyone, 24 h after the challenge if the verifier did not resolve (Challenged → Accepted). */
export const resolveTimeoutTx = (goalId, index) => call(goalId, "resolve_timeout", { index });
/** Anyone, after the deadline of a milestone without a proof (Pending → Failed). */
export const expireTx = (goalId, index) => call(goalId, "expire", { index });
/** Creator: pay out everything claimable. */
export const claimTx = (goalId) => call(goalId, "claim");
// ---- Helpers ----
function bytesToBase64(bytes) {
    let binary = "";
    for (const b of bytes)
        binary += String.fromCharCode(b);
    return btoa(binary);
}

/**
 * Push airdrop: the treasury sends to every address on a list. Nobody claims.
 *
 *   pnpm airdrop:push:devnet recipients.json --dry-run
 *   pnpm airdrop:push:devnet recipients.json
 *   pnpm airdrop:push:mainnet recipients.csv --tokens --limit 10 --yes
 *
 * The pull airdrop (`sleepagotchi_airdrop`, `scripts/claim/`) publishes a Merkle
 * root into a program and waits for people to come and get it. This is the other
 * shape: no program, no vault, no proof, no receipt account — plain SPL transfers
 * signed by the wallet holding the tokens, batched a few recipients to a
 * transaction. Recipients do nothing and pay nothing; the sender pays the fee and
 * the ~0.002 SOL of rent for every recipient that has no $SLEEP account yet.
 *
 * The list is `<address>,<amount>` per line, or the same `{ "<address>":
 * "<amount>" }` JSON that `pnpm tree` takes as a snapshot — so one file can be
 * pushed or published as a root without being rewritten. Amounts are base units
 * unless `--tokens` is passed.
 *
 * Every confirmed transfer is written to a ledger under `out/`, keyed by
 * recipient, and a re-run skips whatever is already in it. That is the whole
 * recovery story: a run that dies at recipient 900 of 4,000 — rate limit, laptop
 * lid, expired blockhash — is finished by running the same command again. The
 * ledger also pins the amount each address was paid, so an edited list cannot
 * quietly top someone up.
 */

import { readFileSync, mkdirSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";

import {
  createAssociatedTokenAccountIdempotentInstruction,
  createTransferCheckedInstruction,
  getAccount,
  getAssociatedTokenAddressSync,
} from "@solana/spl-token";
import {
  ComputeBudgetProgram,
  Connection,
  Keypair,
  PublicKey,
  Transaction,
  type TransactionInstruction,
} from "@solana/web3.js";
import bs58 from "bs58";

import { loadDeployer, parseCluster, readArtifact, toBaseUnits, toDisplay } from "./common";
import { CLUSTERS } from "./config";

/** One 1232-byte packet, minus the signature the message does not carry yet. */
const PACKET_LIMIT = 1232;
/** A transaction that fits is still capped: a big batch fails as one unit. */
const MAX_PER_TX = 10;
const RETRIES = 3;
/** The public RPCs rate-limit a few thousand transfers hard enough to matter. */
const BATCH_DELAY_MS = 250;
/** Rent for a token account, and the fee for a batch — both only estimated here. */
const TOKEN_ACCOUNT_SIZE = 165;
const LAMPORTS_PER_SIGNATURE = 5_000;

type Payment = { recipient: PublicKey; amount: bigint };
type Ledger = Record<string, { amount: string; signature: string }>;

const flag = (name: string) => process.argv.includes(`--${name}`);

/** A numeric flag value, or the fallback. Never silently NaN: `--limit x` sends nothing. */
const number = (name: string, fallback: number): number => {
  const index = process.argv.indexOf(`--${name}`);
  if (index === -1) return fallback;

  const value = Number(process.argv[index + 1]);
  if (!Number.isFinite(value) || value < 0) {
    throw new Error(`--${name} needs a number, got "${process.argv[index + 1] ?? ""}".`);
  }

  return value;
};

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/**
 * `address,amount` lines, or the snapshot object `pnpm tree` reads. Blank lines,
 * `#` comments and a header row are dropped; anything else that does not parse
 * throws with its line number rather than being skipped, because a recipient
 * silently dropped from an airdrop is invisible until someone complains.
 */
const parseList = (path: string, decimals: number, tokens: boolean): Payment[] => {
  const raw = readFileSync(path, "utf8");
  const seen = new Map<string, string>();

  const rows: [string, string, string][] = raw.trimStart().startsWith("{")
    ? Object.entries(JSON.parse(raw) as Record<string, string | number>).map(
        ([address, amount]) => [address, String(amount), `"${address}"`],
      )
    : raw
        .split("\n")
        .map((line, i) => [line.trim(), i + 1] as const)
        .filter(([line]) => line !== "" && !line.startsWith("#"))
        .map(([line, lineNumber]) => {
          const [address = "", amount = ""] = line
            .split(/[,\t;]/)
            .map((cell) => cell.trim().replace(/^"|"$/g, ""));

          return [address, amount, `line ${lineNumber}`] as [string, string, string];
        })
        .filter(([address]) => !/^(address|wallet|recipient|pubkey)$/i.test(address));

  return rows.map(([address, amount, where]) => {
    let recipient: PublicKey;
    try {
      recipient = new PublicKey(address);
    } catch {
      throw new Error(`${where}: "${address}" is not a Solana address.`);
    }

    const previous = seen.get(address);
    if (previous !== undefined) {
      throw new Error(`${where}: ${address} also appears at ${previous}. Merge the two amounts.`);
    }
    seen.set(address, where);

    if (tokens) return { recipient, amount: toBaseUnits(amount, decimals) };

    if (!/^\d+$/.test(amount)) {
      throw new Error(
        `${where}: "${amount}" is not a whole number of base units. Pass --tokens if the list is in whole tokens.`,
      );
    }
    return { recipient, amount: BigInt(amount) };
  });
};

const readLedger = (path: string): Ledger => {
  try {
    return JSON.parse(readFileSync(path, "utf8")) as Ledger;
  } catch {
    return {};
  }
};

const main = async () => {
  const cluster = parseCluster();
  const config = CLUSTERS[cluster];
  const listPath = process.argv[3];
  const dryRun = flag("dry-run");
  const limit = number("limit", Number.POSITIVE_INFINITY);
  const priorityFee = number("priority", 0);

  if (!listPath || listPath.startsWith("--")) {
    throw new Error(
      `Usage: pnpm airdrop:push:${cluster === "mainnet-beta" ? "mainnet" : cluster} <list.csv|list.json> [--tokens] [--limit N] [--priority <microLamports>] [--dry-run] [--yes]`,
    );
  }

  const artifact = readArtifact(cluster);
  const mint = new PublicKey(artifact.mint);
  const decimals = artifact.decimals;
  const display = (base: bigint) => `${toDisplay(base, decimals)} ${artifact.symbol}`;

  const payments = parseList(listPath, decimals, flag("tokens"));
  if (payments.length === 0) throw new Error(`${listPath} lists no recipients.`);

  const zero = payments.find(({ amount }) => amount === 0n);
  if (zero) throw new Error(`${zero.recipient.toBase58()} is allocated nothing. Remove the row.`);

  // A list of whole tokens read as base units pays everyone a millionth of what
  // they are owed, in transfers too small to notice and too many to reverse. The
  // giveaway is a whole list that adds up to less than one token.
  if (!flag("tokens") && !flag("base-units") && payments.reduce((sum, { amount }) => sum + amount, 0n) < 10n ** BigInt(decimals)) {
    throw new Error(
      `The whole list comes to less than one ${artifact.symbol}, read as base units. Pass --tokens if it is in whole ${artifact.symbol}, or --base-units to confirm it is not.`,
    );
  }

  const deployer = loadDeployer();
  if (payments.some(({ recipient }) => recipient.equals(deployer.publicKey))) {
    throw new Error("The list includes the sending wallet, which would pay itself.");
  }

  // Not the treasury check `transfer.ts` makes: where the treasury is a Squads
  // multisig it cannot sign four thousand transactions, so it funds this wallet
  // with a tranche in one proposal and this pushes from there.
  const connection = new Connection(process.env.SOLANA_RPC_URL || config.rpcUrl, "confirmed");
  const source = getAssociatedTokenAddressSync(mint, deployer.publicKey);

  const ledgerPath = join("out", `airdrop-${cluster}-${basename(listPath).replace(/\.[^.]+$/, "")}.json`);
  const ledger = readLedger(ledgerPath);

  for (const { recipient, amount } of payments) {
    const paid = ledger[recipient.toBase58()];
    if (paid && BigInt(paid.amount) !== amount) {
      throw new Error(
        `${recipient.toBase58()} was already sent ${display(BigInt(paid.amount))} (${paid.signature}) but the list now says ${display(amount)}. Reconcile ${ledgerPath} against the list before re-running.`,
      );
    }
  }

  const pending = payments.filter(({ recipient }) => !ledger[recipient.toBase58()]).slice(0, limit);
  const total = payments.reduce((sum, { amount }) => sum + amount, 0n);
  const owed = pending.reduce((sum, { amount }) => sum + amount, 0n);

  // Which recipients still need a token account, so the rent is a number before
  // the run rather than a surprise at recipient 300.
  const destinations = pending.map(({ recipient }) => getAssociatedTokenAddressSync(mint, recipient, true));
  const missing = new Set<string>();
  for (let i = 0; i < destinations.length; i += 100) {
    const slice = destinations.slice(i, i + 100);
    const infos = await connection.getMultipleAccountsInfo(slice);
    infos.forEach((info, j) => {
      if (!info) missing.add(slice[j].toBase58());
    });
  }

  const held = await getAccount(connection, source)
    .then(({ amount }) => amount)
    .catch(() => 0n);
  const lamports = await connection.getBalance(deployer.publicKey);
  const rent = await connection.getMinimumBalanceForRentExemption(TOKEN_ACCOUNT_SIZE);
  const estimatedSol = missing.size * rent + Math.ceil(pending.length / MAX_PER_TX) * LAMPORTS_PER_SIGNATURE;

  function build(batch: Payment[]): TransactionInstruction[] {
    const instructions: TransactionInstruction[] = priorityFee
      ? [ComputeBudgetProgram.setComputeUnitPrice({ microLamports: priorityFee })]
      : [];

    for (const { recipient, amount } of batch) {
      const destination = getAssociatedTokenAddressSync(mint, recipient, true);
      if (missing.has(destination.toBase58())) {
        instructions.push(
          createAssociatedTokenAccountIdempotentInstruction(deployer.publicKey, destination, recipient, mint),
        );
      }
      instructions.push(
        createTransferCheckedInstruction(source, mint, destination, deployer.publicKey, amount, decimals),
      );
    }

    return instructions;
  }

  function fits(batch: Payment[]): boolean {
    const probe = new Transaction({
      feePayer: deployer.publicKey,
      // Any well-formed blockhash: only the compiled size is being measured.
      blockhash: PublicKey.default.toBase58(),
      lastValidBlockHeight: 0,
    }).add(...build(batch));

    return probe.serializeMessage().length + 1 + 64 <= PACKET_LIMIT;
  }

  // One transaction's worth of instructions, packed by measuring the compiled
  // message rather than guessing: a batch whose accounts all need creating fits
  // fewer recipients than one where they all exist.
  const batches: Payment[][] = [];
  for (const payment of pending) {
    const last = batches[batches.length - 1];
    if (last && last.length < MAX_PER_TX && fits([...last, payment])) {
      last.push(payment);
    } else {
      batches.push([payment]);
    }
  }

  console.log(`push airdrop  ${cluster}`);
  console.log(`mint          ${mint.toBase58()}`);
  console.log(`from          ${deployer.publicKey.toBase58()}`);
  console.log(`list          ${listPath}`);
  console.log(`ledger        ${ledgerPath}`);
  console.log();
  console.log(`recipients    ${payments.length} (${payments.length - pending.length} already sent, ${pending.length} to go)`);
  console.log(`transactions  ${batches.length}`);
  console.log(`list total    ${display(total)}`);
  console.log(`to send       ${display(owed)}`);
  console.log(`holding       ${display(held)}`);
  console.log(`new accounts  ${missing.size} (rent ${(missing.size * rent) / 1e9} SOL)`);
  console.log(`sol           ${lamports / 1e9} held, ~${estimatedSol / 1e9} needed`);

  if (pending.length === 0) {
    console.log("\nevery recipient on the list has been paid — nothing to do.");
    return;
  }

  // Reported rather than thrown on a dry run: the point of a dry run is to see
  // the whole plan, including what it will cost to be able to send it.
  const short = [
    held < owed ? `${display(owed - held)}` : null,
    lamports < estimatedSol ? `${(estimatedSol - lamports) / 1e9} SOL of fees and rent` : null,
  ].filter(Boolean);

  if (short.length > 0) {
    const message = `short by ${short.join(" and ")} — fund ${deployer.publicKey.toBase58()}.`;
    if (!dryRun) throw new Error(`\n${message[0].toUpperCase()}${message.slice(1)}`);
    console.log(`\n${message}`);
  }

  if (dryRun) {
    console.log(`\ndry run — ${pending.length} transfers in ${batches.length} transactions, nothing sent.`);
    return;
  }
  if (cluster === "mainnet-beta" && !flag("yes")) {
    throw new Error("Refusing to send on mainnet without --yes. Run it with --dry-run first.");
  }

  console.log(`\nsending ${pending.length} transfers in ${batches.length} transactions`);

  let sent = 0n;
  try {
    for (const [i, batch] of batches.entries()) {
      const signature = await send(build(batch), deployer, connection);

      for (const { recipient, amount } of batch) {
        ledger[recipient.toBase58()] = { amount: amount.toString(), signature };
        sent += amount;
      }
      mkdirSync("out", { recursive: true });
      writeFileSync(ledgerPath, `${JSON.stringify(ledger, null, 2)}\n`);

      console.log(`  ${i + 1}/${batches.length}  ${batch.length} transfers  ${signature}`);
      if (i < batches.length - 1) await sleep(BATCH_DELAY_MS);
    }
  } finally {
    const paid = Object.keys(ledger).length;
    console.log(`\nsent ${display(sent)} to ${paid} of ${payments.length} recipients, ledger at ${ledgerPath}.`);
    if (paid < payments.length) {
      console.log(`${payments.length - paid} left — re-run the same command to finish them.`);
    }
  }
};

/**
 * A confirmation that times out is not a transfer that failed. Signing up front
 * gives the signature before the send, so a batch whose confirmation never
 * arrives can be looked up rather than assumed lost and paid twice.
 */
const send = async (
  instructions: TransactionInstruction[],
  deployer: Keypair,
  connection: Connection,
  attempt = 0,
): Promise<string> => {
  const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash("confirmed");
  const transaction = new Transaction({
    feePayer: deployer.publicKey,
    blockhash,
    lastValidBlockHeight,
  }).add(...instructions);
  transaction.sign(deployer);

  const signature = bs58.encode(transaction.signature!);

  try {
    await connection.sendRawTransaction(transaction.serialize(), { maxRetries: 5 });
    await connection.confirmTransaction({ signature, blockhash, lastValidBlockHeight }, "confirmed");

    return signature;
  } catch (error) {
    const [status] = (await connection.getSignatureStatuses([signature])).value;
    if (status?.confirmationStatus && !status.err) return signature;

    if (attempt >= RETRIES) throw error;
    await sleep(1_000 * (attempt + 1));

    // A new blockhash, so this is a different transaction: the one above either
    // never landed or landed with an error, and neither moved any tokens.
    return send(instructions, deployer, connection, attempt + 1);
  }
};

main().catch((error: unknown) => {
  console.error(`\n${error instanceof Error ? error.message : String(error)}`);
  process.exit(1);
});

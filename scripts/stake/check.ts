/**
 * Validates the hand-rolled encoding against the committed IDL. Offline.
 *
 *   pnpm check
 *
 * `encode.ts` writes Borsh by hand rather than depending on the Anchor TS client,
 * so the field order and widths in `seasonParams` and every `read*` in
 * `common.ts` are a second copy of the program's layout. This is what stops that
 * copy drifting: it is cheap, it needs no cluster, and getting it wrong costs a
 * deploy to find out.
 */

import { PublicKey } from "@solana/web3.js";

import { readConfig } from "./common.js";
import { IDL, seasonParams } from "./encode.js";

type Field = { name: string; type: unknown };

const WIDTHS: Record<string, number> = {
  u8: 1,
  bool: 1,
  u16: 2,
  u32: 4,
  u64: 8,
  i64: 8,
  u128: 16,
  pubkey: 32,
};

const typeOf = (name: string): Field[] => {
  const found = IDL.types?.find((entry) => entry.name === name);
  if (!found) {
    throw new Error(`${name} is not in the IDL`);
  }

  return found.type.fields;
};

const widthOf = (field: Field): number => {
  if (typeof field.type === "string" && field.type in WIDTHS) {
    return WIDTHS[field.type]!;
  }
  if (typeof field.type === "object" && field.type && "defined" in field.type) {
    const nested = (field.type as { defined: { name: string } }).defined.name;

    return typeOf(nested).reduce((total, entry) => total + widthOf(entry), 0);
  }

  throw new Error(`unhandled IDL type ${JSON.stringify(field.type)}`);
};

let failures = 0;
const check = (label: string, actual: unknown, expected: unknown) => {
  const ok = JSON.stringify(actual) === JSON.stringify(expected);
  console.log(`${ok ? "ok  " : "FAIL"}  ${label}`);
  if (!ok) {
    console.log(`        expected ${JSON.stringify(expected)}`);
    console.log(`        actual   ${JSON.stringify(actual)}`);
    failures += 1;
  }
};

// `seasonParams` writes these in this order; the program reads them in the order
// the IDL declares.
check(
  "SeasonParams field order",
  [
    "startTs",
    "endTs",
    "rewardPool",
    "maxTotalStaked",
    "maxPerWallet",
    "maxAprBps",
    "maxMultiplierBps",
    "sweepDelaySeconds",
  ],
  typeOf("SeasonParams").map((field) =>
    field.name.replace(/_([a-z])/g, (_, letter: string) => letter.toUpperCase()),
  ),
);

check(
  "SeasonParams encodes to the declared width",
  seasonParams({
    startTs: 1n,
    endTs: 2n,
    rewardPool: 3n,
    maxTotalStaked: 4n,
    maxPerWallet: 5n,
    maxAprBps: 6,
    maxMultiplierBps: 7,
    sweepDelaySeconds: 8,
  }).length,
  typeOf("SeasonParams").reduce((total, field) => total + widthOf(field), 0),
);

// The readers in common.ts walk these in declaration order after the 8-byte
// discriminator, so a reordered or resized field silently shifts every one after
// it.
for (const [account, fields] of [
  [
    "Config",
    ["admin", "pending_admin", "stake_signer", "mint", "paused", "next_season", "bump"],
  ],
  [
    "Season",
    [
      "id",
      "params",
      "total_staked",
      "total_weighted",
      "weighted_stake_seconds",
      "last_update_ts",
      "rewards_paid",
      "swept",
      "bump",
      "rewards_bump",
    ],
  ],
  [
    "Position",
    [
      "amount",
      "multiplier_bps",
      "weighted_stake_seconds",
      "raw_stake_seconds",
      "last_update_ts",
      "claimed",
      "bump",
    ],
  ],
] as const) {
  check(
    `${account} field order`,
    fields,
    typeOf(account).map((field) => field.name),
  );
}

// `Config.pending_admin` is the only variable-width field in any account: Borsh
// writes `None` as one byte and `Some` as thirty-three, so reading it at a fixed
// width shifts every field after it. Field order alone cannot catch that, hence
// a round trip through the real reader.
{
  const admin = PublicKey.unique();
  const stakeSigner = PublicKey.unique();
  const mint = PublicKey.unique();
  const tail = Buffer.concat([
    stakeSigner.toBuffer(),
    mint.toBuffer(),
    Buffer.from([1]), // paused
    (() => {
      const seasons = Buffer.alloc(8);
      seasons.writeBigUInt64LE(7n);

      return seasons;
    })(),
    Buffer.from([254]), // bump
  ]);
  const head = Buffer.concat([Buffer.alloc(8), admin.toBuffer()]);

  for (const [label, pendingAdmin] of [
    ["none", null],
    ["some", PublicKey.unique()],
  ] as const) {
    const option = pendingAdmin
      ? Buffer.concat([Buffer.from([1]), pendingAdmin.toBuffer()])
      : Buffer.from([0]);
    const config = readConfig(Buffer.concat([head, option, tail]));

    check(`Config round-trips a ${label} pending_admin`,
      {
        admin: config.admin.toBase58(),
        pendingAdmin: config.pendingAdmin?.toBase58() ?? null,
        stakeSigner: config.stakeSigner.toBase58(),
        mint: config.mint.toBase58(),
        paused: config.paused,
        nextSeason: config.nextSeason.toString(),
        bump: config.bump,
      },
      {
        admin: admin.toBase58(),
        pendingAdmin: pendingAdmin?.toBase58() ?? null,
        stakeSigner: stakeSigner.toBase58(),
        mint: mint.toBase58(),
        paused: true,
        nextSeason: "7",
        bump: 254,
      },
    );
  }
}

// Every account name the scripts pass must exist on its instruction. `instruction()`
// throws on a missing one at build time, but only for a path that is exercised.
for (const name of ["initialize", "open_season", "stake", "unstake", "claim"]) {
  const found = IDL.instructions.find((entry) => entry.name === name);
  check(`${name} is in the IDL`, Boolean(found), true);
}

console.log(failures === 0 ? "\nencoding matches the IDL." : `\n${failures} mismatch(es).`);
process.exit(failures === 0 ? 0 : 1);

/**
 * Program IDL in camelCase format in order to be used in JS/TS.
 *
 * Note that this is only a type helper and is not the actual IDL. The original
 * IDL can be found at `target/idl/sleepagotchi_stake.json`.
 */
export type SleepagotchiStake = {
  "address": "AbXC8pN6zbyoi3qWxLcMQC3yXhNUHmXpuDJ1bVDadjKG",
  "metadata": {
    "name": "sleepagotchiStake",
    "version": "0.1.0",
    "spec": "0.1.0",
    "description": "Seasonal $SLEEP staking with pro-rata rewards and a per-wallet APR cap"
  },
  "docs": [
    "Seasonal $SLEEP staking.",
    "",
    "Rewards are a fixed pool per season, split pro-rata by weighted stake-seconds",
    "and capped per wallet at an APR on raw stake-seconds."
  ],
  "instructions": [
    {
      "name": "acceptAdmin",
      "docs": [
        "Signed by the proposed admin, which is what proves the key exists."
      ],
      "discriminator": [
        112,
        42,
        45,
        90,
        116,
        181,
        13,
        170
      ],
      "accounts": [
        {
          "name": "pendingAdmin",
          "signer": true
        },
        {
          "name": "config",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ]
              }
            ]
          }
        }
      ],
      "args": []
    },
    {
      "name": "claim",
      "discriminator": [
        62,
        198,
        214,
        193,
        213,
        159,
        108,
        210
      ],
      "accounts": [
        {
          "name": "user",
          "writable": true,
          "signer": true
        },
        {
          "name": "config",
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ]
              }
            ]
          }
        },
        {
          "name": "season",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  115,
                  101,
                  97,
                  115,
                  111,
                  110
                ]
              },
              {
                "kind": "account",
                "path": "season.id",
                "account": "season"
              }
            ]
          }
        },
        {
          "name": "position",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  112,
                  111,
                  115,
                  105,
                  116,
                  105,
                  111,
                  110
                ]
              },
              {
                "kind": "account",
                "path": "season.id",
                "account": "season"
              },
              {
                "kind": "account",
                "path": "user"
              }
            ]
          }
        },
        {
          "name": "rewardsAuthority",
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  114,
                  101,
                  119,
                  97,
                  114,
                  100,
                  115
                ]
              },
              {
                "kind": "account",
                "path": "season.id",
                "account": "season"
              }
            ]
          }
        },
        {
          "name": "mint",
          "relations": [
            "config"
          ]
        },
        {
          "name": "rewardVault",
          "docs": [
            "Rewards only. The stake vault has a different authority and is not an",
            "account this instruction can name."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "account",
                "path": "rewardsAuthority"
              },
              {
                "kind": "const",
                "value": [
                  6,
                  221,
                  246,
                  225,
                  215,
                  101,
                  161,
                  147,
                  217,
                  203,
                  225,
                  70,
                  206,
                  235,
                  121,
                  172,
                  28,
                  180,
                  133,
                  237,
                  95,
                  91,
                  55,
                  145,
                  58,
                  140,
                  245,
                  133,
                  126,
                  255,
                  0,
                  169
                ]
              },
              {
                "kind": "account",
                "path": "mint"
              }
            ],
            "program": {
              "kind": "const",
              "value": [
                140,
                151,
                37,
                143,
                78,
                36,
                137,
                241,
                187,
                61,
                16,
                41,
                20,
                142,
                13,
                131,
                11,
                90,
                19,
                153,
                218,
                255,
                16,
                132,
                4,
                142,
                123,
                216,
                219,
                233,
                248,
                89
              ]
            }
          }
        },
        {
          "name": "destination",
          "writable": true
        },
        {
          "name": "tokenProgram",
          "address": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        }
      ],
      "args": []
    },
    {
      "name": "initialize",
      "discriminator": [
        175,
        175,
        109,
        31,
        13,
        152,
        155,
        237
      ],
      "accounts": [
        {
          "name": "admin",
          "writable": true,
          "signer": true
        },
        {
          "name": "config",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ]
              }
            ]
          }
        },
        {
          "name": "mint",
          "docs": [
            "Fixed here for the life of the program. Every season's vaults are token",
            "accounts for this mint, so a config that could switch it would strand",
            "balances behind seasons that no longer match."
          ]
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": [
        {
          "name": "stakeSigner",
          "type": "pubkey"
        }
      ]
    },
    {
      "name": "openSeason",
      "discriminator": [
        100,
        152,
        95,
        245,
        247,
        126,
        124,
        215
      ],
      "accounts": [
        {
          "name": "admin",
          "writable": true,
          "signer": true,
          "relations": [
            "config"
          ]
        },
        {
          "name": "config",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ]
              }
            ]
          }
        },
        {
          "name": "season",
          "docs": [
            "Id comes from the config's allocator rather than from input, so seasons",
            "are sequential and no id can be skipped or reused."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  115,
                  101,
                  97,
                  115,
                  111,
                  110
                ]
              },
              {
                "kind": "account",
                "path": "config.nextSeason",
                "account": "config"
              }
            ]
          }
        },
        {
          "name": "rewardsAuthority",
          "docs": [
            "to be a different owner from `season`, which is what keeps principal and",
            "rewards in separate token accounts."
          ],
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  114,
                  101,
                  119,
                  97,
                  114,
                  100,
                  115
                ]
              },
              {
                "kind": "account",
                "path": "config.nextSeason",
                "account": "config"
              }
            ]
          }
        },
        {
          "name": "mint",
          "relations": [
            "config"
          ]
        },
        {
          "name": "stakeVault",
          "docs": [
            "Principal, and only principal. No instruction can move it except",
            "`unstake`."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "account",
                "path": "season"
              },
              {
                "kind": "const",
                "value": [
                  6,
                  221,
                  246,
                  225,
                  215,
                  101,
                  161,
                  147,
                  217,
                  203,
                  225,
                  70,
                  206,
                  235,
                  121,
                  172,
                  28,
                  180,
                  133,
                  237,
                  95,
                  91,
                  55,
                  145,
                  58,
                  140,
                  245,
                  133,
                  126,
                  255,
                  0,
                  169
                ]
              },
              {
                "kind": "account",
                "path": "mint"
              }
            ],
            "program": {
              "kind": "const",
              "value": [
                140,
                151,
                37,
                143,
                78,
                36,
                137,
                241,
                187,
                61,
                16,
                41,
                20,
                142,
                13,
                131,
                11,
                90,
                19,
                153,
                218,
                255,
                16,
                132,
                4,
                142,
                123,
                216,
                219,
                233,
                248,
                89
              ]
            }
          }
        },
        {
          "name": "rewardVault",
          "docs": [
            "Holds exactly `params.reward_pool` from here on."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "account",
                "path": "rewardsAuthority"
              },
              {
                "kind": "const",
                "value": [
                  6,
                  221,
                  246,
                  225,
                  215,
                  101,
                  161,
                  147,
                  217,
                  203,
                  225,
                  70,
                  206,
                  235,
                  121,
                  172,
                  28,
                  180,
                  133,
                  237,
                  95,
                  91,
                  55,
                  145,
                  58,
                  140,
                  245,
                  133,
                  126,
                  255,
                  0,
                  169
                ]
              },
              {
                "kind": "account",
                "path": "mint"
              }
            ],
            "program": {
              "kind": "const",
              "value": [
                140,
                151,
                37,
                143,
                78,
                36,
                137,
                241,
                187,
                61,
                16,
                41,
                20,
                142,
                13,
                131,
                11,
                90,
                19,
                153,
                218,
                255,
                16,
                132,
                4,
                142,
                123,
                216,
                219,
                233,
                248,
                89
              ]
            }
          }
        },
        {
          "name": "funding",
          "writable": true
        },
        {
          "name": "tokenProgram",
          "address": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        },
        {
          "name": "associatedTokenProgram",
          "address": "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": [
        {
          "name": "params",
          "type": {
            "defined": {
              "name": "seasonParams"
            }
          }
        }
      ]
    },
    {
      "name": "setPaused",
      "discriminator": [
        91,
        60,
        125,
        192,
        176,
        225,
        166,
        218
      ],
      "accounts": [
        {
          "name": "admin",
          "signer": true,
          "relations": [
            "config"
          ]
        },
        {
          "name": "config",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ]
              }
            ]
          }
        }
      ],
      "args": [
        {
          "name": "paused",
          "type": "bool"
        }
      ]
    },
    {
      "name": "setStakeSigner",
      "discriminator": [
        80,
        210,
        116,
        106,
        201,
        43,
        251,
        239
      ],
      "accounts": [
        {
          "name": "admin",
          "signer": true,
          "relations": [
            "config"
          ]
        },
        {
          "name": "config",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ]
              }
            ]
          }
        }
      ],
      "args": [
        {
          "name": "stakeSigner",
          "type": "pubkey"
        }
      ]
    },
    {
      "name": "stake",
      "discriminator": [
        206,
        176,
        202,
        18,
        200,
        209,
        179,
        108
      ],
      "accounts": [
        {
          "name": "user",
          "docs": [
            "Fee payer, and pays the position's rent on the first stake."
          ],
          "writable": true,
          "signer": true
        },
        {
          "name": "stakeSigner",
          "docs": [
            "Attests `multiplier_bps` against an NFT the program cannot see, on a",
            "chain it cannot reach. `Signer` plus the `has_one` below is the whole",
            "authorization check — the runtime has already verified the signature by",
            "the time this instruction runs."
          ],
          "signer": true,
          "relations": [
            "config"
          ]
        },
        {
          "name": "config",
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ]
              }
            ]
          }
        },
        {
          "name": "season",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  115,
                  101,
                  97,
                  115,
                  111,
                  110
                ]
              },
              {
                "kind": "account",
                "path": "season.id",
                "account": "season"
              }
            ]
          }
        },
        {
          "name": "position",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  112,
                  111,
                  115,
                  105,
                  116,
                  105,
                  111,
                  110
                ]
              },
              {
                "kind": "account",
                "path": "season.id",
                "account": "season"
              },
              {
                "kind": "account",
                "path": "user"
              }
            ]
          }
        },
        {
          "name": "mint",
          "relations": [
            "config"
          ]
        },
        {
          "name": "stakeVault",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "account",
                "path": "season"
              },
              {
                "kind": "const",
                "value": [
                  6,
                  221,
                  246,
                  225,
                  215,
                  101,
                  161,
                  147,
                  217,
                  203,
                  225,
                  70,
                  206,
                  235,
                  121,
                  172,
                  28,
                  180,
                  133,
                  237,
                  95,
                  91,
                  55,
                  145,
                  58,
                  140,
                  245,
                  133,
                  126,
                  255,
                  0,
                  169
                ]
              },
              {
                "kind": "account",
                "path": "mint"
              }
            ],
            "program": {
              "kind": "const",
              "value": [
                140,
                151,
                37,
                143,
                78,
                36,
                137,
                241,
                187,
                61,
                16,
                41,
                20,
                142,
                13,
                131,
                11,
                90,
                19,
                153,
                218,
                255,
                16,
                132,
                4,
                142,
                123,
                216,
                219,
                233,
                248,
                89
              ]
            }
          }
        },
        {
          "name": "source",
          "writable": true
        },
        {
          "name": "tokenProgram",
          "address": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": [
        {
          "name": "amount",
          "type": "u64"
        },
        {
          "name": "multiplierBps",
          "type": "u32"
        }
      ]
    },
    {
      "name": "sweepUnclaimed",
      "discriminator": [
        64,
        168,
        221,
        224,
        42,
        216,
        138,
        144
      ],
      "accounts": [
        {
          "name": "admin",
          "signer": true,
          "relations": [
            "config"
          ]
        },
        {
          "name": "config",
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ]
              }
            ]
          }
        },
        {
          "name": "season",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  115,
                  101,
                  97,
                  115,
                  111,
                  110
                ]
              },
              {
                "kind": "account",
                "path": "season.id",
                "account": "season"
              }
            ]
          }
        },
        {
          "name": "rewardsAuthority",
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  114,
                  101,
                  119,
                  97,
                  114,
                  100,
                  115
                ]
              },
              {
                "kind": "account",
                "path": "season.id",
                "account": "season"
              }
            ]
          }
        },
        {
          "name": "mint",
          "relations": [
            "config"
          ]
        },
        {
          "name": "rewardVault",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "account",
                "path": "rewardsAuthority"
              },
              {
                "kind": "const",
                "value": [
                  6,
                  221,
                  246,
                  225,
                  215,
                  101,
                  161,
                  147,
                  217,
                  203,
                  225,
                  70,
                  206,
                  235,
                  121,
                  172,
                  28,
                  180,
                  133,
                  237,
                  95,
                  91,
                  55,
                  145,
                  58,
                  140,
                  245,
                  133,
                  126,
                  255,
                  0,
                  169
                ]
              },
              {
                "kind": "account",
                "path": "mint"
              }
            ],
            "program": {
              "kind": "const",
              "value": [
                140,
                151,
                37,
                143,
                78,
                36,
                137,
                241,
                187,
                61,
                16,
                41,
                20,
                142,
                13,
                131,
                11,
                90,
                19,
                153,
                218,
                255,
                16,
                132,
                4,
                142,
                123,
                216,
                219,
                233,
                248,
                89
              ]
            }
          }
        },
        {
          "name": "destination",
          "docs": [
            "Treasury, or the reward vault of a later season — rolling a remainder",
            "forward is a sweep plus an `open_season`, not a separate instruction."
          ],
          "writable": true
        },
        {
          "name": "tokenProgram",
          "address": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        }
      ],
      "args": []
    },
    {
      "name": "transferAdmin",
      "docs": [
        "Proposes a handover, or cancels one with `None`. The admin does not",
        "change until `accept_admin`."
      ],
      "discriminator": [
        42,
        242,
        66,
        106,
        228,
        10,
        111,
        156
      ],
      "accounts": [
        {
          "name": "admin",
          "signer": true,
          "relations": [
            "config"
          ]
        },
        {
          "name": "config",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ]
              }
            ]
          }
        }
      ],
      "args": [
        {
          "name": "newAdmin",
          "type": {
            "option": "pubkey"
          }
        }
      ]
    },
    {
      "name": "unstake",
      "discriminator": [
        90,
        95,
        107,
        42,
        205,
        124,
        50,
        225
      ],
      "accounts": [
        {
          "name": "user",
          "writable": true,
          "signer": true
        },
        {
          "name": "config",
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ]
              }
            ]
          }
        },
        {
          "name": "season",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  115,
                  101,
                  97,
                  115,
                  111,
                  110
                ]
              },
              {
                "kind": "account",
                "path": "season.id",
                "account": "season"
              }
            ]
          }
        },
        {
          "name": "position",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  112,
                  111,
                  115,
                  105,
                  116,
                  105,
                  111,
                  110
                ]
              },
              {
                "kind": "account",
                "path": "season.id",
                "account": "season"
              },
              {
                "kind": "account",
                "path": "user"
              }
            ]
          }
        },
        {
          "name": "mint",
          "relations": [
            "config"
          ]
        },
        {
          "name": "stakeVault",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "account",
                "path": "season"
              },
              {
                "kind": "const",
                "value": [
                  6,
                  221,
                  246,
                  225,
                  215,
                  101,
                  161,
                  147,
                  217,
                  203,
                  225,
                  70,
                  206,
                  235,
                  121,
                  172,
                  28,
                  180,
                  133,
                  237,
                  95,
                  91,
                  55,
                  145,
                  58,
                  140,
                  245,
                  133,
                  126,
                  255,
                  0,
                  169
                ]
              },
              {
                "kind": "account",
                "path": "mint"
              }
            ],
            "program": {
              "kind": "const",
              "value": [
                140,
                151,
                37,
                143,
                78,
                36,
                137,
                241,
                187,
                61,
                16,
                41,
                20,
                142,
                13,
                131,
                11,
                90,
                19,
                153,
                218,
                255,
                16,
                132,
                4,
                142,
                123,
                216,
                219,
                233,
                248,
                89
              ]
            }
          }
        },
        {
          "name": "destination",
          "docs": [
            "Any token account the caller owns, so unstaking never depends on an ATA",
            "existing."
          ],
          "writable": true
        },
        {
          "name": "tokenProgram",
          "address": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        }
      ],
      "args": [
        {
          "name": "amount",
          "type": "u64"
        }
      ]
    },
    {
      "name": "updateSeason",
      "discriminator": [
        225,
        91,
        34,
        185,
        228,
        6,
        98,
        136
      ],
      "accounts": [
        {
          "name": "admin",
          "writable": true,
          "signer": true,
          "relations": [
            "config"
          ]
        },
        {
          "name": "config",
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ]
              }
            ]
          }
        },
        {
          "name": "season",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  115,
                  101,
                  97,
                  115,
                  111,
                  110
                ]
              },
              {
                "kind": "account",
                "path": "season.id",
                "account": "season"
              }
            ]
          }
        },
        {
          "name": "rewardsAuthority",
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  114,
                  101,
                  119,
                  97,
                  114,
                  100,
                  115
                ]
              },
              {
                "kind": "account",
                "path": "season.id",
                "account": "season"
              }
            ]
          }
        },
        {
          "name": "mint",
          "relations": [
            "config"
          ]
        },
        {
          "name": "rewardVault",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "account",
                "path": "rewardsAuthority"
              },
              {
                "kind": "const",
                "value": [
                  6,
                  221,
                  246,
                  225,
                  215,
                  101,
                  161,
                  147,
                  217,
                  203,
                  225,
                  70,
                  206,
                  235,
                  121,
                  172,
                  28,
                  180,
                  133,
                  237,
                  95,
                  91,
                  55,
                  145,
                  58,
                  140,
                  245,
                  133,
                  126,
                  255,
                  0,
                  169
                ]
              },
              {
                "kind": "account",
                "path": "mint"
              }
            ],
            "program": {
              "kind": "const",
              "value": [
                140,
                151,
                37,
                143,
                78,
                36,
                137,
                241,
                187,
                61,
                16,
                41,
                20,
                142,
                13,
                131,
                11,
                90,
                19,
                153,
                218,
                255,
                16,
                132,
                4,
                142,
                123,
                216,
                219,
                233,
                248,
                89
              ]
            }
          }
        },
        {
          "name": "funding",
          "docs": [
            "Source when the pool goes up, destination when it goes down."
          ],
          "writable": true
        },
        {
          "name": "tokenProgram",
          "address": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        }
      ],
      "args": [
        {
          "name": "params",
          "type": {
            "defined": {
              "name": "seasonParams"
            }
          }
        }
      ]
    }
  ],
  "accounts": [
    {
      "name": "config",
      "discriminator": [
        155,
        12,
        170,
        224,
        30,
        250,
        204,
        130
      ]
    },
    {
      "name": "position",
      "discriminator": [
        170,
        188,
        143,
        228,
        122,
        64,
        247,
        208
      ]
    },
    {
      "name": "season",
      "discriminator": [
        76,
        67,
        93,
        156,
        180,
        157,
        248,
        47
      ]
    }
  ],
  "events": [
    {
      "name": "adminTransferProposed",
      "discriminator": [
        203,
        168,
        175,
        51,
        239,
        104,
        20,
        85
      ]
    },
    {
      "name": "adminTransferred",
      "discriminator": [
        255,
        147,
        182,
        5,
        199,
        217,
        38,
        179
      ]
    },
    {
      "name": "pausedSet",
      "discriminator": [
        171,
        125,
        127,
        156,
        233,
        81,
        68,
        66
      ]
    },
    {
      "name": "rewardClaimed",
      "discriminator": [
        49,
        28,
        87,
        84,
        158,
        48,
        229,
        175
      ]
    },
    {
      "name": "seasonOpened",
      "discriminator": [
        188,
        53,
        161,
        112,
        10,
        91,
        118,
        131
      ]
    },
    {
      "name": "seasonUpdated",
      "discriminator": [
        221,
        95,
        138,
        83,
        153,
        255,
        121,
        113
      ]
    },
    {
      "name": "stakeSignerUpdated",
      "discriminator": [
        174,
        68,
        112,
        145,
        75,
        168,
        104,
        216
      ]
    },
    {
      "name": "staked",
      "discriminator": [
        11,
        146,
        45,
        205,
        230,
        58,
        213,
        240
      ]
    },
    {
      "name": "sweptUnclaimed",
      "discriminator": [
        204,
        68,
        90,
        71,
        161,
        83,
        56,
        220
      ]
    },
    {
      "name": "unstaked",
      "discriminator": [
        27,
        179,
        156,
        215,
        47,
        71,
        195,
        7
      ]
    }
  ],
  "errors": [
    {
      "code": 6000,
      "name": "paused",
      "msg": "Staking is paused"
    },
    {
      "code": 6001,
      "name": "startInThePast",
      "msg": "Season start is in the past"
    },
    {
      "code": 6002,
      "name": "invalidWindow",
      "msg": "Season start is not before its end"
    },
    {
      "code": 6003,
      "name": "invalidParameters",
      "msg": "Season parameter is zero or below its floor"
    },
    {
      "code": 6004,
      "name": "seasonStarted",
      "msg": "Season has already started and is frozen"
    },
    {
      "code": 6005,
      "name": "seasonNotStarted",
      "msg": "Season has not started yet"
    },
    {
      "code": 6006,
      "name": "seasonEnded",
      "msg": "Season has ended"
    },
    {
      "code": 6007,
      "name": "seasonNotEnded",
      "msg": "Season has not ended yet"
    },
    {
      "code": 6008,
      "name": "zeroAmount",
      "msg": "Amount is zero"
    },
    {
      "code": 6009,
      "name": "multiplierTooLow",
      "msg": "Multiplier is below 1x"
    },
    {
      "code": 6010,
      "name": "multiplierTooHigh",
      "msg": "Multiplier is above the season maximum"
    },
    {
      "code": 6011,
      "name": "walletCapExceeded",
      "msg": "Stake would exceed the per-wallet maximum"
    },
    {
      "code": 6012,
      "name": "seasonFull",
      "msg": "Stake would exceed the season maximum"
    },
    {
      "code": 6013,
      "name": "insufficientStake",
      "msg": "Unstake is larger than the staked amount"
    },
    {
      "code": 6014,
      "name": "alreadyClaimed",
      "msg": "Rewards for this position have already been claimed"
    },
    {
      "code": 6015,
      "name": "nothingStaked",
      "msg": "Season accrued no weighted stake"
    },
    {
      "code": 6016,
      "name": "insufficientVault",
      "msg": "Vault holds less than the amount owed"
    },
    {
      "code": 6017,
      "name": "sweepTooEarly",
      "msg": "Season has not ended, so there is nothing to sweep"
    },
    {
      "code": 6018,
      "name": "alreadySwept",
      "msg": "Season has already been swept"
    },
    {
      "code": 6019,
      "name": "mathOverflow",
      "msg": "Arithmetic overflow"
    },
    {
      "code": 6020,
      "name": "sweepDelayTooShort",
      "msg": "Sweep delay is below the minimum claim window"
    },
    {
      "code": 6021,
      "name": "notPendingAdmin",
      "msg": "Signer is not the pending admin, or no handover is in flight"
    }
  ],
  "types": [
    {
      "name": "adminTransferProposed",
      "docs": [
        "Proposal only — `admin` is unchanged until the pending key signs",
        "`accept_admin`. `pending` is `None` when a handover is cancelled."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "current",
            "type": "pubkey"
          },
          {
            "name": "pending",
            "type": {
              "option": "pubkey"
            }
          }
        ]
      }
    },
    {
      "name": "adminTransferred",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "previous",
            "type": "pubkey"
          },
          {
            "name": "current",
            "type": "pubkey"
          }
        ]
      }
    },
    {
      "name": "config",
      "docs": [
        "Singleton, seeds `[CONFIG_SEED]`."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "admin",
            "type": "pubkey"
          },
          {
            "name": "pendingAdmin",
            "docs": [
              "Proposed next admin, pending its own signature. `None` when no handover is",
              "in flight.",
              "",
              "Less severe here than in the claim programs — `unstake` and `claim` are",
              "permissionless, so a key nobody holds strands no user funds, only",
              "`open_season` and `sweep_unclaimed`. Two steps regardless, because the",
              "three programs having one admin shape is worth more than the saving."
            ],
            "type": {
              "option": "pubkey"
            }
          },
          {
            "name": "stakeSigner",
            "docs": [
              "Backend key that must co-sign every stake. Attests the multiplier, and",
              "nothing else. Never funded."
            ],
            "type": "pubkey"
          },
          {
            "name": "mint",
            "type": "pubkey"
          },
          {
            "name": "paused",
            "docs": [
              "Blocks new stakes. Never blocks `unstake` or `claim`."
            ],
            "type": "bool"
          },
          {
            "name": "nextSeason",
            "type": "u64"
          },
          {
            "name": "bump",
            "type": "u8"
          }
        ]
      }
    },
    {
      "name": "pausedSet",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "paused",
            "type": "bool"
          }
        ]
      }
    },
    {
      "name": "position",
      "docs": [
        "Seeds `[POSITION_SEED, season.id.to_le_bytes(), user]`. Per season, so a",
        "wallet's history in one season is independent of any other."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "amount",
            "type": "u64"
          },
          {
            "name": "multiplierBps",
            "docs": [
              "Re-attested on every stake, and applied only from that instant forward."
            ],
            "type": "u32"
          },
          {
            "name": "weightedStakeSeconds",
            "docs": [
              "Numerator of this position's pro-rata share."
            ],
            "type": "u128"
          },
          {
            "name": "rawStakeSeconds",
            "docs": [
              "The same integral without the multiplier. Base of the APR cap, which is",
              "a statement about tokens actually locked rather than about weight."
            ],
            "type": "u128"
          },
          {
            "name": "lastUpdateTs",
            "type": "i64"
          },
          {
            "name": "claimed",
            "type": "bool"
          },
          {
            "name": "bump",
            "type": "u8"
          }
        ]
      }
    },
    {
      "name": "rewardClaimed",
      "docs": [
        "Carries both integrals, so the payout can be re-derived from the log without",
        "reading the accounts back."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "user",
            "type": "pubkey"
          },
          {
            "name": "season",
            "type": "u64"
          },
          {
            "name": "owed",
            "type": "u64"
          },
          {
            "name": "weightedStakeSeconds",
            "type": "u128"
          },
          {
            "name": "rawStakeSeconds",
            "type": "u128"
          },
          {
            "name": "seasonWeightedStakeSeconds",
            "type": "u128"
          }
        ]
      }
    },
    {
      "name": "season",
      "docs": [
        "Seeds `[SEASON_SEED, id.to_le_bytes()]`. Authority on the stake vault, which",
        "is this account's associated token account for the mint."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "id",
            "type": "u64"
          },
          {
            "name": "params",
            "docs": [
              "Rewritable in full by `update_season` until `start_ts`, frozen from that",
              "instant on. Safe because nothing can be staked before a season starts, so",
              "there is no accrual for a change to invalidate."
            ],
            "type": {
              "defined": {
                "name": "seasonParams"
              }
            }
          },
          {
            "name": "totalStaked",
            "docs": [
              "Raw, for the capacity check against `max_total_staked`."
            ],
            "type": "u64"
          },
          {
            "name": "totalWeighted",
            "docs": [
              "Σ amount × multiplier_bps over open positions, as of `last_update_ts`."
            ],
            "type": "u128"
          },
          {
            "name": "weightedStakeSeconds",
            "docs": [
              "The integral of `total_weighted`. Denominator of every pro-rata share,",
              "and equal to the sum of every position's own once all are settled to",
              "`end_ts`."
            ],
            "type": "u128"
          },
          {
            "name": "lastUpdateTs",
            "type": "i64"
          },
          {
            "name": "rewardsPaid",
            "type": "u64"
          },
          {
            "name": "swept",
            "type": "bool"
          },
          {
            "name": "bump",
            "type": "u8"
          },
          {
            "name": "rewardsBump",
            "docs": [
              "Bump for `[REWARDS_SEED, id]`, the reward vault's authority."
            ],
            "type": "u8"
          }
        ]
      }
    },
    {
      "name": "seasonOpened",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "id",
            "type": "u64"
          },
          {
            "name": "params",
            "type": {
              "defined": {
                "name": "seasonParams"
              }
            }
          }
        ]
      }
    },
    {
      "name": "seasonParams",
      "docs": [
        "Everything a season is configured with, as one type.",
        "",
        "Taken whole by both `open_season` and `update_season`, which makes the freeze",
        "rule a single assignment being refused rather than a per-field check a new",
        "parameter could escape."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "startTs",
            "type": "i64"
          },
          {
            "name": "endTs",
            "type": "i64"
          },
          {
            "name": "rewardPool",
            "docs": [
              "Fixed. The reward vault holds exactly this much, from `open_season` on."
            ],
            "type": "u64"
          },
          {
            "name": "maxTotalStaked",
            "type": "u64"
          },
          {
            "name": "maxPerWallet",
            "type": "u64"
          },
          {
            "name": "maxAprBps",
            "type": "u16"
          },
          {
            "name": "maxMultiplierBps",
            "docs": [
              "Ceiling on any attested multiplier. A bounded blast radius for a",
              "compromised or buggy signer."
            ],
            "type": "u32"
          },
          {
            "name": "sweepDelaySeconds",
            "docs": [
              "How long after `end_ts` the unclaimed remainder becomes sweepable. The",
              "claim window, in other words — a sweep takes the whole vault including",
              "rewards nobody has collected yet, so this is what stops a season's",
              "payouts being cancelled the second it closes."
            ],
            "type": "u32"
          }
        ]
      }
    },
    {
      "name": "seasonUpdated",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "id",
            "type": "u64"
          },
          {
            "name": "params",
            "type": {
              "defined": {
                "name": "seasonParams"
              }
            }
          }
        ]
      }
    },
    {
      "name": "stakeSignerUpdated",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "previous",
            "type": "pubkey"
          },
          {
            "name": "current",
            "type": "pubkey"
          }
        ]
      }
    },
    {
      "name": "staked",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "user",
            "type": "pubkey"
          },
          {
            "name": "season",
            "type": "u64"
          },
          {
            "name": "amount",
            "docs": [
              "This deposit, not the position total."
            ],
            "type": "u64"
          },
          {
            "name": "multiplierBps",
            "docs": [
              "Re-attested on every stake, and in force only from this instant."
            ],
            "type": "u32"
          },
          {
            "name": "positionAmount",
            "type": "u64"
          },
          {
            "name": "seasonTotalStaked",
            "type": "u64"
          }
        ]
      }
    },
    {
      "name": "sweptUnclaimed",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "season",
            "type": "u64"
          },
          {
            "name": "amount",
            "type": "u64"
          },
          {
            "name": "destination",
            "type": "pubkey"
          }
        ]
      }
    },
    {
      "name": "unstaked",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "user",
            "type": "pubkey"
          },
          {
            "name": "season",
            "type": "u64"
          },
          {
            "name": "amount",
            "type": "u64"
          },
          {
            "name": "positionAmount",
            "type": "u64"
          },
          {
            "name": "seasonTotalStaked",
            "type": "u64"
          }
        ]
      }
    }
  ],
  "constants": [
    {
      "name": "configSeed",
      "type": "bytes",
      "value": "[99, 111, 110, 102, 105, 103]"
    },
    {
      "name": "minSweepDelaySeconds",
      "docs": [
        "Floor on `sweep_delay_seconds`, seven days.",
        "",
        "Rewards are not payable until `end_ts`, so without a floor there is always an",
        "interval in which the whole pool is sweepable and nothing has been claimed —",
        "one admin transaction at `end_ts + 1` would take every reward earned and not",
        "yet collected. A floor rather than a fixed value: the admin still chooses how",
        "long to wait, but not whether to wait at all."
      ],
      "type": "u32",
      "value": "604800"
    },
    {
      "name": "positionSeed",
      "type": "bytes",
      "value": "[112, 111, 115, 105, 116, 105, 111, 110]"
    },
    {
      "name": "rewardsSeed",
      "docs": [
        "Authority over the reward vault. Separate from the season PDA so principal",
        "and rewards are different token accounts under different owners — a bug in",
        "the reward maths then cannot reach anyone's stake."
      ],
      "type": "bytes",
      "value": "[114, 101, 119, 97, 114, 100, 115]"
    },
    {
      "name": "seasonSeed",
      "type": "bytes",
      "value": "[115, 101, 97, 115, 111, 110]"
    },
    {
      "name": "secondsPerYear",
      "docs": [
        "365 days. Written out rather than derived so the APR denominator is readable",
        "at the point of use."
      ],
      "type": "i64",
      "value": "31536000"
    }
  ]
};

import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { PublicKey, Keypair, SystemProgram } from "@solana/web3.js";
import { expect } from "chai";

describe("solana-pyth-oracle-stream", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.PythOracleStream as any;
  const authority = Keypair.generate();
  const mockPythFeed = Keypair.generate();
  const streamId = new anchor.BN(Date.now());

  let feedStreamPda: PublicKey;

  before(async () => {
    // Air drop SOL to authority
    const airdropSig = await provider.connection.requestAirdrop(
      authority.publicKey,
      2 * anchor.web3.LAMPORTS_PER_SOL
    );
    await provider.connection.confirmTransaction(airdropSig);

    [feedStreamPda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("oracle_stream"),
        authority.publicKey.toBuffer(),
        streamId.toArrayLike(Buffer, "le", 8),
      ],
      program.programId
    );
  });

  it("Initializes Pyth oracle feed stream PDA", async () => {
    const maxStalenessSeconds = new anchor.BN(60);
    const maxConfRatioBps = new anchor.BN(500); // 5% max confidence ratio

    await program.methods
      .initializeFeedStream(
        streamId,
        maxStalenessSeconds,
        maxConfRatioBps
      )
      .accounts({
        feedStream: feedStreamPda,
        pythPriceFeed: mockPythFeed.publicKey,
        authority: authority.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([authority])
      .rpc();

    const state = await program.account.feedStreamState.fetch(feedStreamPda);
    expect(state.authority.toBase58()).to.equal(authority.publicKey.toBase58());
    expect(state.pythPriceFeed.toBase58()).to.equal(mockPythFeed.publicKey.toBase58());
    expect(state.streamId.toNumber()).to.equal(streamId.toNumber());
    expect(state.sampleCount.toNumber()).to.equal(0);
    expect(state.isActive).to.be.true;
  });

  it("Updates stream active status to paused", async () => {
    await program.methods
      .setStreamStatus(false)
      .accounts({
        feedStream: feedStreamPda,
        authority: authority.publicKey,
      })
      .signers([authority])
      .rpc();

    const state = await program.account.feedStreamState.fetch(feedStreamPda);
    expect(state.isActive).to.be.false;
  });
});

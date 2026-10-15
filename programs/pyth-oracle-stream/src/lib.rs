use anchor_lang::prelude::*;
use pyth_sdk_solana::load_price_feed_from_account_info;

declare_id!("PythStrm11111111111111111111111111111111111");

pub const MAX_PRICE_HISTORY_SAMPLES: usize = 16;

#[program]
pub mod pyth_oracle_stream {
    use super::*;

    /// Initializes a price stream tracking a specific Pyth Network feed
    pub fn initialize_feed_stream(
        ctx: Context<InitializeFeedStream>,
        stream_id: u64,
        max_staleness_seconds: u64,
        max_conf_ratio_bps: u64,
    ) -> Result<()> {
        let stream = &mut ctx.accounts.feed_stream;
        stream.authority = ctx.accounts.authority.key();
        stream.pyth_price_feed = ctx.accounts.pyth_price_feed.key();
        stream.stream_id = stream_id;
        stream.max_staleness_seconds = max_staleness_seconds;
        stream.max_conf_ratio_bps = max_conf_ratio_bps;
        stream.sample_count = 0;
        stream.cumulative_price_sum = 0;
        stream.latest_twap = 0;
        stream.latest_price = 0;
        stream.latest_timestamp = 0;
        stream.is_active = true;

        emit!(StreamInitialized {
            stream_id,
            authority: stream.authority,
            pyth_price_feed: stream.pyth_price_feed,
        });

        Ok(())
    }

    /// Pulls current price from Pyth, validates confidence & staleness, and updates the TWAP accumulator
    pub fn record_price_sample(ctx: Context<RecordPriceSample>) -> Result<()> {
        let stream = &mut ctx.accounts.feed_stream;
        require!(stream.is_active, OracleStreamError::StreamPaused);

        let clock = Clock::get()?;
        let current_time = clock.unix_timestamp;

        // Load and parse Pyth price feed
        let price_feed_account = &ctx.accounts.pyth_price_feed;
        let price_feed = load_price_feed_from_account_info(price_feed_account)
            .map_err(|_| OracleStreamError::InvalidPythFeed)?;

        // Ensure price is not older than configured staleness
        let price_data = price_feed
            .get_price_no_older_than(current_time, stream.max_staleness_seconds)
            .ok_or(OracleStreamError::PriceFeedStale)?;

        require!(price_data.price > 0, OracleStreamError::NonPositivePrice);

        // Confidence interval ratio check (conf / price <= max_conf_ratio_bps / 10000)
        let conf_ratio_bps = (price_data.conf as u128)
            .checked_mul(10_000)
            .and_then(|v| v.checked_div(price_data.price as u128))
            .unwrap_or(u128::MAX);

        require!(
            conf_ratio_bps <= stream.max_conf_ratio_bps as u128,
            OracleStreamError::ConfidenceIntervalTooWide
        );

        let price_val = price_data.price as u64;

        // Update accumulator and rolling TWAP
        stream.cumulative_price_sum = stream
            .cumulative_price_sum
            .checked_add(price_val as u128)
            .ok_or(OracleStreamError::MathOverflow)?;

        stream.sample_count = stream.sample_count.saturating_add(1);
        stream.latest_price = price_val;
        stream.latest_timestamp = current_time;
        stream.latest_twap = (stream.cumulative_price_sum / stream.sample_count as u128) as u64;

        emit!(PriceSampleRecorded {
            stream_id: stream.stream_id,
            price: price_val,
            conf: price_data.conf,
            twap: stream.latest_twap,
            timestamp: current_time,
        });

        Ok(())
    }

    /// Sets active/paused status of oracle feed stream
    pub fn set_stream_status(ctx: Context<UpdateStreamConfig>, is_active: bool) -> Result<()> {
        let stream = &mut ctx.accounts.feed_stream;
        stream.is_active = is_active;
        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(stream_id: u64)]
pub struct InitializeFeedStream<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + FeedStreamState::LEN,
        seeds = [b"oracle_stream", authority.key().as_ref(), &stream_id.to_le_bytes()],
        bump
    )]
    pub feed_stream: Account<'info, FeedStreamState>,
    /// CHECK: Pyth price feed account validated by Pyth SDK
    pub pyth_price_feed: AccountInfo<'info>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct RecordPriceSample<'info> {
    #[account(
        mut,
        has_one = pyth_price_feed,
        seeds = [b"oracle_stream", feed_stream.authority.as_ref(), &feed_stream.stream_id.to_le_bytes()],
        bump
    )]
    pub feed_stream: Account<'info, FeedStreamState>,
    /// CHECK: Pyth price feed account
    pub pyth_price_feed: AccountInfo<'info>,
    pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct UpdateStreamConfig<'info> {
    #[account(
        mut,
        has_one = authority,
        seeds = [b"oracle_stream", authority.key().as_ref(), &feed_stream.stream_id.to_le_bytes()],
        bump
    )]
    pub feed_stream: Account<'info, FeedStreamState>,
    pub authority: Signer<'info>,
}

#[account]
pub struct FeedStreamState {
    pub authority: Pubkey,
    pub pyth_price_feed: Pubkey,
    pub stream_id: u64,
    pub max_staleness_seconds: u64,
    pub max_conf_ratio_bps: u64,
    pub sample_count: u64,
    pub cumulative_price_sum: u128,
    pub latest_twap: u64,
    pub latest_price: u64,
    pub latest_timestamp: i64,
    pub is_active: bool,
}

impl FeedStreamState {
    pub const LEN: usize = 32 // authority
        + 32 // pyth_price_feed
        + 8  // stream_id
        + 8  // max_staleness_seconds
        + 8  // max_conf_ratio_bps
        + 8  // sample_count
        + 16 // cumulative_price_sum
        + 8  // latest_twap
        + 8  // latest_price
        + 8  // latest_timestamp
        + 1  // is_active
        + 64; // reserved padding
}

#[event]
pub struct StreamInitialized {
    pub stream_id: u64,
    pub authority: Pubkey,
    pub pyth_price_feed: Pubkey,
}

#[event]
pub struct PriceSampleRecorded {
    pub stream_id: u64,
    pub price: u64,
    pub conf: u64,
    pub twap: u64,
    pub timestamp: i64,
}

#[error_code]
pub enum OracleStreamError {
    #[msg("Provided account is not a valid Pyth Network price feed")]
    InvalidPythFeed,
    #[msg("Pyth price feed is older than max allowed staleness")]
    PriceFeedStale,
    #[msg("Oracle price must be strictly positive")]
    NonPositivePrice,
    #[msg("Price confidence interval is too wide compared to price")]
    ConfidenceIntervalTooWide,
    #[msg("Feed stream is currently paused")]
    StreamPaused,
    #[msg("Arithmetic overflow in accumulator")]
    MathOverflow,
}

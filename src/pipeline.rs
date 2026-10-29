use crate::{
    Catalog, DecisionComparison, LocateTrace, LogicalRecord, MatchBackend, MatchRequest,
    OracleEvaluation, P0dError, ReconstructedRecord, compare, locate, oracle,
};
use crate::{
    fetch_stage::{Fetch, FetchPreparation, FetchTimings, elapsed},
    match_stage,
};
use serde::Serialize;
use serde_json::Value;
use std::time::Instant;

#[derive(Clone, Debug, Serialize)]
pub struct PreparationTimings {
    pub catalog_and_layout_ns: u64,
    pub fetch: FetchPreparation,
    pub total_ns: u64,
}
#[derive(Clone, Debug, Serialize)]
pub struct RequestTimings {
    pub match_ns: u64,
    pub locate_ns: u64,
    pub fetch_ns: u64,
    pub fetch: FetchTimings,
    pub request_ns: u64,
    pub derived_online_sum_ns: u64,
    pub independent_oracle_ns: u64,
}
#[derive(Debug, Serialize)]
pub struct Run {
    pub match_backend: MatchBackend,
    pub since_prepare_start_through_fetch_ns: Option<u64>,
    pub scores: Vec<f64>,
    pub locate: LocateTrace,
    pub output: Option<ReconstructedRecord>,
    pub fetch_count: usize,
    pub oracle: OracleEvaluation,
    pub decisions: Vec<DecisionComparison>,
    pub exact_selection_agreement: bool,
    pub semantic_selection_agreement: bool,
    pub ckks_metrics: Value,
    pub timings: RequestTimings,
}
pub struct Pipeline {
    match_backend: MatchBackend,
    catalog: Catalog,
    fetch: Fetch,
    pub preparation: PreparationTimings,
    cold_start: Option<Instant>,
}
impl Pipeline {
    pub fn prepare(records: Vec<LogicalRecord>) -> Result<Self, P0dError> {
        Self::prepare_with_match_backend(records, MatchBackend::CtCt)
    }
    pub fn prepare_with_match_backend(
        records: Vec<LogicalRecord>,
        match_backend: MatchBackend,
    ) -> Result<Self, P0dError> {
        let start = Instant::now();
        let cstart = Instant::now();
        let catalog = Catalog::new(records)?;
        let catalog_and_layout_ns = elapsed(cstart);
        let (fetch, t) = Fetch::prepare(&catalog)?;
        Ok(Self {
            match_backend,
            catalog,
            fetch,
            preparation: PreparationTimings {
                catalog_and_layout_ns,
                fetch: t,
                total_ns: elapsed(start),
            },
            cold_start: Some(start),
        })
    }
    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }
    pub fn run(&mut self, q: &MatchRequest) -> Result<Run, P0dError> {
        self.catalog.validate(q)?;
        let request_start = Instant::now();
        let start = Instant::now();
        let m = match_stage::run_with_backend(&self.catalog, q, self.match_backend)?;
        let match_ns = elapsed(start);
        let start = Instant::now();
        let trace = locate(&self.catalog, q, &m.scores, &m.positions)?;
        let locate_ns = elapsed(start);
        let start = Instant::now();
        let (output, fetch_t) = self.fetch.run(&self.catalog, trace.target)?;
        let fetch_ns = elapsed(start);
        let request_ns = elapsed(request_start);
        let cold = self.cold_start.take().map(elapsed);
        let derived_online_sum_ns = m.metrics.derived_online_match_ns()?
            + locate_ns
            + fetch_t.query_ns
            + fetch_t.server_ns
            + fetch_t.decrypt_decode_ns;
        let start = Instant::now();
        let o = oracle(&self.catalog, q)?;
        let decisions = compare(&o, &m.scores, q.threshold)?;
        let exact_selection_agreement = output == o.exact_record;
        let semantic_selection_agreement = output == o.semantic_record;
        let independent_oracle_ns = elapsed(start);
        Ok(Run {
            match_backend: self.match_backend,
            since_prepare_start_through_fetch_ns: cold,
            scores: m.scores,
            locate: trace,
            output,
            fetch_count: 1,
            oracle: o,
            decisions,
            exact_selection_agreement,
            semantic_selection_agreement,
            ckks_metrics: match_stage::metrics(&m.metrics),
            timings: RequestTimings {
                match_ns,
                locate_ns,
                fetch_ns,
                fetch: fetch_t,
                request_ns,
                derived_online_sum_ns,
                independent_oracle_ns,
            },
        })
    }
    pub fn serialization_probe(&mut self) -> Result<Value, P0dError> {
        self.fetch.sizes(&self.catalog)
    }
}

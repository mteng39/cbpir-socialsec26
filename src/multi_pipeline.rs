use crate::{
    Catalog, DecisionComparison, LogicalRecord, MatchBackend, MatchRequest, MultiLocateTrace,
    OracleEvaluation, P0dError, ReconstructedRecord, compare, locate_many, oracle, oracle_records,
    validate_multi_public_k,
};
use crate::{
    fetch_stage::elapsed,
    match_stage,
    multi_fetch_stage::{MultiFetch, MultiFetchPreparation, MultiFetchTimings, MultiFetchTrace},
};
use serde::Serialize;
use serde_json::Value;
use std::time::Instant;

#[derive(Clone, Debug, Serialize)]
pub struct MultiPreparationTimings {
    pub catalog_and_layout_ns: u64,
    pub fetch: MultiFetchPreparation,
    pub total_ns: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct MultiRequestTimings {
    pub match_ns: u64,
    pub locate_ns: u64,
    pub fetch_ns: u64,
    pub fetch: MultiFetchTimings,
    pub request_ns: u64,
    pub derived_online_sum_ns: u64,
    pub independent_oracle_ns: u64,
}

#[derive(Debug, Serialize)]
pub struct MultiRun {
    pub match_backend: MatchBackend,
    pub since_prepare_start_through_fetch_ns: Option<u64>,
    pub public_k: usize,
    pub scores: Vec<f64>,
    pub locate: MultiLocateTrace,
    pub fetch: MultiFetchTrace,
    pub outputs: Vec<ReconstructedRecord>,
    pub fetch_count: usize,
    pub oracle: OracleEvaluation,
    pub oracle_exact_outputs: Vec<ReconstructedRecord>,
    pub oracle_semantic_outputs: Vec<ReconstructedRecord>,
    pub decisions: Vec<DecisionComparison>,
    pub exact_selection_agreement: bool,
    pub semantic_selection_agreement: bool,
    pub ckks_metrics: Value,
    pub timings: MultiRequestTimings,
}

pub struct MultiPipeline {
    match_backend: MatchBackend,
    public_k: usize,
    catalog: Catalog,
    fetch: MultiFetch,
    pub preparation: MultiPreparationTimings,
    cold_start: Option<Instant>,
}

impl MultiPipeline {
    pub fn prepare(records: Vec<LogicalRecord>, public_k: usize) -> Result<Self, P0dError> {
        Self::prepare_with_match_backend(records, public_k, MatchBackend::CtCt)
    }

    pub fn prepare_with_match_backend(
        records: Vec<LogicalRecord>,
        public_k: usize,
        match_backend: MatchBackend,
    ) -> Result<Self, P0dError> {
        validate_multi_public_k(public_k)?;
        let started = Instant::now();
        let catalog_started = Instant::now();
        let catalog = Catalog::new(records)?;
        let catalog_and_layout_ns = elapsed(catalog_started);
        let (fetch, fetch_preparation) = MultiFetch::prepare(&catalog, public_k)?;
        Ok(Self {
            match_backend,
            public_k,
            catalog,
            fetch,
            preparation: MultiPreparationTimings {
                catalog_and_layout_ns,
                fetch: fetch_preparation,
                total_ns: elapsed(started),
            },
            cold_start: Some(started),
        })
    }

    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    pub const fn public_k(&self) -> usize {
        self.public_k
    }

    pub fn run(&mut self, request: &MatchRequest) -> Result<MultiRun, P0dError> {
        self.catalog.validate(request)?;
        let request_started = Instant::now();

        let started = Instant::now();
        let matched = match_stage::run_with_backend(&self.catalog, request, self.match_backend)?;
        let match_ns = elapsed(started);

        let started = Instant::now();
        let locate = locate_many(
            &self.catalog,
            request,
            &matched.scores,
            &matched.positions,
            self.public_k,
        )?;
        let locate_ns = elapsed(started);

        let started = Instant::now();
        let (outputs, fetch, fetch_timings) =
            self.fetch.run(&self.catalog, &locate.selected_candidates)?;
        let fetch_ns = elapsed(started);
        let request_ns = elapsed(request_started);
        let since_prepare_start_through_fetch_ns = self.cold_start.take().map(elapsed);

        let derived_online_sum_ns = matched.metrics.derived_online_match_ns()?
            + locate_ns
            + fetch_timings.query_ns
            + fetch_timings.server_ns
            + fetch_timings.decrypt_decode_ns;

        let started = Instant::now();
        let oracle = oracle(&self.catalog, request)?;
        let oracle_exact_outputs = oracle_records(&self.catalog, &oracle.exact_ids, self.public_k)?;
        let oracle_semantic_outputs =
            oracle_records(&self.catalog, &oracle.semantic_ids, self.public_k)?;
        let decisions = compare(&oracle, &matched.scores, request.threshold)?;
        let exact_selection_agreement = outputs == oracle_exact_outputs;
        let semantic_selection_agreement = outputs == oracle_semantic_outputs;
        let independent_oracle_ns = elapsed(started);

        Ok(MultiRun {
            match_backend: self.match_backend,
            since_prepare_start_through_fetch_ns,
            public_k: self.public_k,
            scores: matched.scores,
            locate,
            fetch,
            outputs,
            fetch_count: 1,
            oracle,
            oracle_exact_outputs,
            oracle_semantic_outputs,
            decisions,
            exact_selection_agreement,
            semantic_selection_agreement,
            ckks_metrics: match_stage::metrics(&matched.metrics),
            timings: MultiRequestTimings {
                match_ns,
                locate_ns,
                fetch_ns,
                fetch: fetch_timings,
                request_ns,
                derived_online_sum_ns,
                independent_oracle_ns,
            },
        })
    }
}

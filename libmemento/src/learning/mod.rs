pub mod agent;
pub mod bootstrap;
pub mod confidence;
pub mod curator;
pub mod delta_context;
pub mod detailed_generation;
pub mod error;
pub mod failure_detector;
pub mod generator;
pub mod interaction_tracker;
pub mod nlg;
pub mod reasoning_trace;
pub mod reflector;

pub use agent::{Agent, QueryResponse};
pub use bootstrap::{BootstrapEngine, Document, LearningPhase, SearchResult};
pub use confidence::{compute_query_confidence, compute_query_confidence_cached};
pub use curator::{ACECurator, AdaptiveCurator, Curator, UpdateMetrics, UpdateSuccessTracker};
pub use delta_context::{CooccurrenceTriple, DeltaContextItem, DeltaItemType, DeltaSource};
pub use detailed_generation::{
    ContentPartition, ContentPlanner, DetailedAnswer, DetailedGenerator, DiversityEnforcer,
    ParagraphBuilder, ParagraphSpec, WeightedEvidence,
};
pub use error::{LearningError, Result};
pub use generator::{GeneratedAnswer, Generator};
pub use interaction_tracker::{
    ClickEvent, DistributionMonitor, FeedbackEvent, InteractionEvent, InteractionTracker,
    QueryTrajectory, ReformulationEvent, SessionStatus,
};
pub use nlg::{Answer, Context, ContextBuilder, Evidence, NLGEngine, ResponseFormat, Source};
pub use reasoning_trace::{Citation, EvidenceChunk, ReasoningStep, ReasoningTrace, Span, SubQuery};
pub use reflector::ace_patterns;
pub use reflector::{
    AdaptiveReflector, CooccurrenceCandidate, DisambiguationEdit, DualReflectorSystem,
    EditQualityPredictor, InteractionPattern, Reflector, ReflectorController,
    ReflectorPerformanceTracker, StaticReflector,
};

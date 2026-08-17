use super::*;

impl BootstrapEngine {
    pub(super) fn phase_1_babbling(&mut self) -> Result<()> {
        println!("--- Phase 1: Babbling (β=0.5) ---");
        self.learning_phase = LearningPhase::Babbling;
        let learning_rate = self.learning_phase.learning_rate();

        println!("Ingesting seed corpus...");
        let corpus_len = self.seed_corpus.len();
        for i in 0..corpus_len {
            let doc = self.seed_corpus[i].clone();
            self.ingest_document_with_rate(&doc, learning_rate)?;

            if i % 10 == 0 && i > 0 {
                let coherence = self.measure_coherence()?;
                println!("  Ingested {} docs, coherence: {:.3}", i, coherence);

                if coherence > self.learning_phase.transition_threshold() {
                    println!("  Early transition: coherence > 0.3");
                    return Ok(());
                }
            }
        }

        println!("Generating synthetic queries...");
        let query_count = 500;
        let synthetic_queries = self.generate_synthetic_queries(query_count);

        for (i, query) in synthetic_queries.iter().enumerate() {
            let results = self.execute_query(query)?;

            if !results.is_empty() {
                let clicked_result = self.simulate_click(&results)?;
                self.apply_learning_delta(query, &clicked_result, learning_rate)?;
            }

            if i % 50 == 0 && i > 0 {
                let coherence = self.measure_coherence()?;
                println!("  Query {}/{}, coherence: {:.3}", i, query_count, coherence);

                if coherence > self.learning_phase.transition_threshold() {
                    println!("  Transition: Babbling → FirstWords (coherence > 0.3)");
                    return Ok(());
                }
            }
        }

        Ok(())
    }

    pub(super) fn phase_2_first_words(&mut self) -> Result<()> {
        println!("--- Phase 2: FirstWords (β=0.1) ---");
        self.learning_phase = LearningPhase::FirstWords;
        let learning_rate = self.learning_phase.learning_rate();

        let query_count = 400;
        let synthetic_queries = self.generate_complex_queries(query_count);

        for (i, query) in synthetic_queries.iter().enumerate() {
            let results = self.execute_query(query)?;

            if results.is_empty() {
                self.reflect_on_failure(query, learning_rate)?;
            } else {
                let clicked = self.simulate_click(&results)?;
                self.apply_learning_delta(query, &clicked, learning_rate)?;
            }

            if i % 50 == 0 && i > 0 {
                let coherence = self.measure_coherence()?;
                println!("  Query {}/{}, coherence: {:.3}", i, query_count, coherence);

                if coherence > self.learning_phase.transition_threshold() {
                    println!("  Transition: FirstWords → Grammar (coherence > 0.7)");
                    return Ok(());
                }
            }
        }

        Ok(())
    }

    pub(super) fn phase_3_grammar(&mut self) -> Result<()> {
        println!("--- Phase 3: Grammar (β=0.01) ---");
        self.learning_phase = LearningPhase::Grammar;
        let learning_rate = self.learning_phase.learning_rate();

        let query_count = 100;
        let synthetic_queries = self.generate_cross_domain_queries(query_count);

        for (i, query) in synthetic_queries.iter().enumerate() {
            let results = self.execute_query(query)?;

            if !results.is_empty() {
                let clicked = self.simulate_click(&results)?;
                self.apply_learning_delta(query, &clicked, learning_rate)?;
            }

            if i % 20 == 0 && i > 0 {
                let coherence = self.measure_coherence()?;
                println!("  Query {}/{}, coherence: {:.3}", i, query_count, coherence);
            }
        }

        Ok(())
    }
}

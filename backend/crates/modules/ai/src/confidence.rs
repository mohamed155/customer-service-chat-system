/// Inputs used to compute a confidence score for an AI-generated response.
pub struct ConfidenceInputs {
    pub top_chunk_similarity: f32,
    pub chunk_count: u32,
    pub finish_length: bool,
    pub retrieval_degraded: bool,
    pub continuation_used: bool,
}

fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

/// Returns a score in [0.0, 1.0] indicating how confident the system is in
/// the quality of the AI response, based on retrieval quality, truncation,
/// retrieval failures, and continuation stitching.
pub fn confidence_score(i: &ConfidenceInputs) -> f32 {
    clamp01(
        0.35 + 0.45 * i.top_chunk_similarity + 0.10 * (i.chunk_count.min(3) as f32) / 3.0
            - 0.25 * (i.finish_length as u32 as f32)
            - 0.15 * (i.retrieval_degraded as u32 as f32)
            - 0.10 * (i.continuation_used as u32 as f32),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Band {
    High,
    Medium,
    Low,
}

impl Band {
    pub fn as_str(&self) -> &'static str {
        match self {
            Band::High => "high",
            Band::Medium => "medium",
            Band::Low => "low",
        }
    }
}

pub fn confidence_band(score: f32) -> Band {
    if score >= 0.70 {
        Band::High
    } else if score >= 0.40 {
        Band::Medium
    } else {
        Band::Low
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_grounding_low() {
        let i = ConfidenceInputs {
            top_chunk_similarity: 0.0,
            chunk_count: 0,
            finish_length: false,
            retrieval_degraded: false,
            continuation_used: false,
        };
        let score = confidence_score(&i);
        assert!((score - 0.35).abs() < f32::EPSILON);
        assert_eq!(confidence_band(score), Band::Low);
    }

    #[test]
    fn strong_grounding_high() {
        let i = ConfidenceInputs {
            top_chunk_similarity: 0.9,
            chunk_count: 3,
            finish_length: false,
            retrieval_degraded: false,
            continuation_used: false,
        };
        let score = confidence_score(&i);
        let expected: f32 =
            (0.35_f32 + 0.45_f32 * 0.9_f32 + 0.10_f32 * 3.0_f32 / 3.0_f32).clamp(0.0, 1.0);
        assert!((score - expected).abs() < f32::EPSILON);
        assert_eq!(confidence_band(score), Band::High);
    }

    #[test]
    fn truncated_deducts_025() {
        let base = confidence_score(&ConfidenceInputs {
            top_chunk_similarity: 0.0,
            chunk_count: 0,
            finish_length: false,
            retrieval_degraded: false,
            continuation_used: false,
        });
        let truncated = confidence_score(&ConfidenceInputs {
            top_chunk_similarity: 0.0,
            chunk_count: 0,
            finish_length: true,
            retrieval_degraded: false,
            continuation_used: false,
        });
        assert!((truncated - (base - 0.25)).abs() < f32::EPSILON);
    }

    #[test]
    fn band_high() {
        assert_eq!(confidence_band(0.70), Band::High);
        assert_eq!(confidence_band(1.0), Band::High);
    }

    #[test]
    fn band_medium() {
        assert_eq!(confidence_band(0.69), Band::Medium);
        assert_eq!(confidence_band(0.40), Band::Medium);
    }

    #[test]
    fn band_low() {
        assert_eq!(confidence_band(0.39), Band::Low);
        assert_eq!(confidence_band(0.0), Band::Low);
    }

    #[test]
    fn band_as_str() {
        assert_eq!(Band::High.as_str(), "high");
        assert_eq!(Band::Medium.as_str(), "medium");
        assert_eq!(Band::Low.as_str(), "low");
    }
}

//! Exact, bounded minimum-cardinality cover over declared capability masks.
//!
//! A complete route set has at most four providers. Split any such set into
//! two groups of at most two. We retain the best one-or-two-provider
//! representation for each 16-bit union, then use a superset transform to
//! find a second representation covering each first group's remaining bits.
//! Replacing either half with a smaller equivalent representation cannot
//! increase the union's cardinality, so this finds a minimum-size full cover.
use super::Candidate;
use serde::Serialize;
use std::collections::BTreeMap;

const MASK_COUNT: usize = 1 << 16;
const INVALID_LEN: u8 = u8::MAX;

#[derive(Clone, Copy)]
struct PairChoice {
    first: u16,
    second: u16,
    len: u8,
}

impl PairChoice {
    const INVALID: Self = Self {
        first: u16::MAX,
        second: u16::MAX,
        len: INVALID_LEN,
    };
    const EMPTY: Self = Self {
        first: u16::MAX,
        second: u16::MAX,
        len: 0,
    };

    fn single(index: usize) -> Self {
        Self {
            first: index as u16,
            second: u16::MAX,
            len: 1,
        }
    }

    fn pair(first: usize, second: usize) -> Self {
        Self {
            first: first as u16,
            second: second as u16,
            len: 2,
        }
    }

    fn valid(self) -> bool {
        self.len != INVALID_LEN
    }

    fn indices(self) -> [u16; 2] {
        [self.first, self.second]
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct CoverageSearch {
    pub algorithm_version: &'static str,
    pub full_coverage_feasible: Option<bool>,
    pub eligible_provider_count: usize,
    pub distinct_coverage_masks: usize,
    pub pair_visits: u64,
    pub selected_additional_count: usize,
    pub greedy_fallback: bool,
    pub declaration_only: bool,
    pub permissions_granted: bool,
}

struct Provider {
    id: String,
    mask: u16,
    direct: bool,
}

pub(super) fn select(
    candidates: &BTreeMap<String, Candidate<'_>>,
    requested: &[&String],
    selected: &mut Vec<String>,
    already_covered: u16,
) -> CoverageSearch {
    let target = requested_mask(requested) & !already_covered;
    let mut eligible_provider_count = 0;
    let mut by_mask: BTreeMap<u16, Provider> = BTreeMap::new();
    for (id, candidate) in candidates {
        if !candidate.available || selected.contains(id) || candidate.mask & target == 0 {
            continue;
        }
        eligible_provider_count += 1;
        let provider = Provider {
            id: id.clone(),
            mask: candidate.mask & target,
            direct: requested.iter().any(|capability| capability.as_str() == id),
        };
        match by_mask.get(&provider.mask) {
            Some(existing) if provider_rank(existing) <= provider_rank(&provider) => {}
            _ => {
                by_mask.insert(provider.mask, provider);
            }
        }
    }
    let mut providers = by_mask.into_values().collect::<Vec<_>>();
    providers.sort_by(|left, right| provider_rank(left).cmp(&provider_rank(right)));

    let mut report = CoverageSearch {
        algorithm_version: "exact-cover/1",
        full_coverage_feasible: None,
        eligible_provider_count,
        distinct_coverage_masks: providers.len(),
        pair_visits: 0,
        selected_additional_count: 0,
        greedy_fallback: false,
        declaration_only: true,
        permissions_granted: false,
    };

    if target == 0 {
        report.full_coverage_feasible = Some(true);
        return report;
    }

    let available_union = providers
        .iter()
        .fold(0, |mask, provider| mask | provider.mask);
    let remaining_slots = 4usize.saturating_sub(selected.len());
    let exact = if available_union & target != target || remaining_slots == 0 {
        None
    } else {
        find_minimum_cover(&providers, target, remaining_slots, &mut report.pair_visits)
    };

    if let Some(indices) = exact {
        for index in indices {
            selected.push(providers[index as usize].id.clone());
        }
        report.full_coverage_feasible = Some(true);
        // `selected` includes the retained routes. Count only newly appended IDs.
        report.selected_additional_count = providers
            .iter()
            .filter(|provider| selected.iter().any(|id| id == &provider.id))
            .count();
    } else {
        report.full_coverage_feasible = Some(false);
        report.greedy_fallback = true;
        let initial_len = selected.len();
        let mut covered = already_covered;
        while selected.len() < 4 {
            let next = candidates
                .iter()
                .filter(|(id, candidate)| {
                    candidate.available && !selected.contains(id) && candidate.mask & !covered != 0
                })
                .max_by(|(left_id, left), (right_id, right)| {
                    (left.mask & !covered)
                        .count_ones()
                        .cmp(&(right.mask & !covered).count_ones())
                        .then_with(|| {
                            requested
                                .iter()
                                .any(|capability| capability.as_str() == left.skill.id)
                                .cmp(
                                    &requested
                                        .iter()
                                        .any(|capability| capability.as_str() == right.skill.id),
                                )
                        })
                        .then_with(|| right_id.cmp(left_id))
                });
            let Some((id, candidate)) = next else {
                break;
            };
            selected.push(id.clone());
            covered |= candidate.mask;
        }
        report.selected_additional_count = selected.len() - initial_len;
    }
    report
}

fn provider_rank(provider: &Provider) -> (bool, &str) {
    (!provider.direct, provider.id.as_str())
}

fn requested_mask(requested: &[&String]) -> u16 {
    if requested.len() == 16 {
        u16::MAX
    } else {
        (1u16 << requested.len()) - 1
    }
}

fn find_minimum_cover(
    providers: &[Provider],
    target: u16,
    max_routes: usize,
    pair_visits: &mut u64,
) -> Option<Vec<u16>> {
    if target == 0 {
        return Some(Vec::new());
    }
    if max_routes == 0 {
        return None;
    }
    if let Some(index) = providers
        .iter()
        .position(|provider| provider.mask & target == target)
    {
        return Some(vec![index as u16]);
    }
    if max_routes == 1 {
        return None;
    }
    // A two-route capacity needs neither union tables nor the superset transform.
    if max_routes == 2 {
        for first in 0..providers.len() {
            for second in first + 1..providers.len() {
                *pair_visits += 1;
                if (providers[first].mask | providers[second].mask) & target == target {
                    return Some(vec![first as u16, second as u16]);
                }
            }
        }
        return None;
    }

    let bits = (u16::BITS - target.leading_zeros()) as usize;
    let mask_count = 1usize << bits;
    debug_assert!(mask_count <= MASK_COUNT);
    let mut exact = vec![PairChoice::INVALID; mask_count];
    exact[0] = PairChoice::EMPTY;
    for (index, provider) in providers.iter().enumerate() {
        let choice = PairChoice::single(index);
        insert_better(&mut exact[(provider.mask & target) as usize], choice);
    }
    for first in 0..providers.len() {
        for second in first + 1..providers.len() {
            *pair_visits += 1;
            let union = (providers[first].mask | providers[second].mask) & target;
            if union == target {
                // No single provider covered the target, so two is minimal.
                return Some(vec![first as u16, second as u16]);
            }
            insert_better(&mut exact[union as usize], PairChoice::pair(first, second));
        }
    }

    let mut best_superset = exact.clone();
    for bit in 0..bits {
        let bit_mask = 1usize << bit;
        for mask in 0..mask_count {
            if mask & bit_mask == 0 {
                let covering = best_superset[mask | bit_mask];
                if is_better(covering, best_superset[mask]) {
                    best_superset[mask] = covering;
                }
            }
        }
    }

    let mut best: Option<[u16; 4]> = None;
    let mut best_len = u8::MAX;
    for (first_union, first_choice) in exact.iter().copied().enumerate() {
        if !first_choice.valid() {
            continue;
        }
        let missing = (target as usize) & !first_union;
        let second_choice = best_superset[missing];
        if !second_choice.valid() {
            continue;
        }
        let (indices, len) = merge_choices(first_choice, second_choice);
        if len as usize > max_routes
            || (indices[..len as usize]
                .iter()
                .fold(0u16, |mask, index| mask | providers[*index as usize].mask)
                & target)
                != target
        {
            continue;
        }
        if len < best_len || (len == best_len && best.is_some_and(|old| indices < old)) {
            best = Some(indices);
            best_len = len;
        }
    }
    best.map(|indices| indices[..best_len as usize].to_vec())
}

fn insert_better(slot: &mut PairChoice, candidate: PairChoice) {
    if is_better(candidate, *slot) {
        *slot = candidate;
    }
}

fn is_better(candidate: PairChoice, current: PairChoice) -> bool {
    if !candidate.valid() {
        return false;
    }
    if !current.valid() {
        return true;
    }
    if candidate.len != current.len {
        return candidate.len < current.len;
    }
    candidate.indices() < current.indices()
}

fn merge_choices(left: PairChoice, right: PairChoice) -> ([u16; 4], u8) {
    let mut indices = [u16::MAX; 4];
    let mut len = 0usize;
    for choice in [left, right] {
        for index in choice.indices().into_iter().take(choice.len as usize) {
            if !indices[..len].contains(&index) {
                indices[len] = index;
                len += 1;
            }
        }
    }
    indices[..len].sort_unstable();
    (indices, len as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn providers(masks: &[u16]) -> Vec<Provider> {
        masks
            .iter()
            .enumerate()
            .map(|(index, mask)| Provider {
                id: format!("expert.{index:04}"),
                mask: *mask,
                direct: false,
            })
            .collect()
    }

    fn brute_force_minimum(masks: &[u16], target: u16, max_routes: usize) -> Option<usize> {
        for count in 0..=max_routes {
            let mut found = false;
            combinations(masks, target, count, 0, 0, &mut found);
            if found {
                return Some(count);
            }
        }
        None
    }

    fn combinations(
        masks: &[u16],
        target: u16,
        remaining: usize,
        start: usize,
        covered: u16,
        found: &mut bool,
    ) {
        if *found {
            return;
        }
        if remaining == 0 {
            *found = covered & target == target;
            return;
        }
        for index in start..masks.len() {
            combinations(
                masks,
                target,
                remaining - 1,
                index + 1,
                covered | masks[index],
                found,
            );
        }
    }

    fn exact_size(masks: &[u16], target: u16, max_routes: usize) -> Option<usize> {
        let providers = providers(masks);
        let mut visits = 0;
        find_minimum_cover(&providers, target, max_routes, &mut visits).map(|indices| indices.len())
    }

    #[test]
    fn exact_cover_matches_exhaustive_oracle_for_small_masks_and_limits() {
        for masks in [
            vec![0b0001, 0b0010, 0b0100, 0b1000, 0b0011],
            vec![0b0011, 0b0110, 0b1100, 0b1001, 0b1111],
            vec![0b0001, 0b0010, 0b0100, 0b1000],
            vec![0b0011, 0b1100, 0b0101, 0b1010],
        ] {
            for target in 0..16 {
                for max_routes in 0..=4 {
                    assert_eq!(
                        exact_size(&masks, target, max_routes),
                        brute_force_minimum(&masks, target, max_routes),
                        "masks={masks:?}, target={target:04b}, max_routes={max_routes}"
                    );
                }
            }
        }

        // Every provider family over a three-capability universe, including
        // overlapping and redundant masks, agrees with direct subset search.
        let available_masks = [1u16, 2, 3, 4, 5, 6, 7];
        for family in 0u16..(1 << available_masks.len()) {
            let masks = available_masks
                .iter()
                .enumerate()
                .filter_map(|(index, mask)| (family & (1 << index) != 0).then_some(*mask))
                .collect::<Vec<_>>();
            for target in 0..8 {
                for max_routes in 0..=4 {
                    assert_eq!(
                        exact_size(&masks, target, max_routes),
                        brute_force_minimum(&masks, target, max_routes),
                        "family={family:07b}, target={target:03b}, max_routes={max_routes}"
                    );
                }
            }
        }
    }

    #[test]
    fn minimum_full_cover_beats_a_greedy_dead_end() {
        // Greedy's three-way choices can leave a final singleton uncovered,
        // while the two disjoint providers cover the requested set.
        let masks = [0b0111, 0b1011, 0b1101, 0b1110, 0b1111];
        assert_eq!(exact_size(&masks[..4], 0b1111, 2), Some(2));
        assert_eq!(brute_force_minimum(&masks[..4], 0b1111, 2), Some(2));
    }

    #[test]
    fn sixteen_bit_full_cover_needs_four_five_bit_providers() {
        let masks: [u16; 4] = [
            0b0000_0000_0001_1111,
            0b0000_0001_1110_0001,
            0b0001_1110_0000_0001,
            0b1110_0000_0000_0011,
        ];
        assert_eq!(
            masks
                .iter()
                .map(|mask| mask.count_ones())
                .collect::<Vec<_>>(),
            vec![5; 4]
        );
        assert_eq!(exact_size(&masks, u16::MAX, 4), Some(4));
        assert_eq!(brute_force_minimum(&masks, u16::MAX, 3), None);
    }

    #[test]
    fn small_covers_use_fast_paths_before_constructing_tables() {
        let mut visits = 0;
        assert_eq!(
            find_minimum_cover(&providers(&[3, 1]), 3, 4, &mut visits),
            Some(vec![0])
        );
        assert_eq!(visits, 0);
        assert_eq!(
            find_minimum_cover(&providers(&[1, 2]), 3, 1, &mut visits),
            None
        );
        assert_eq!(visits, 0);
        assert_eq!(
            find_minimum_cover(&providers(&[1, 2]), 3, 2, &mut visits),
            Some(vec![0, 1])
        );
        assert_eq!(visits, 1);
        assert_eq!(
            find_minimum_cover(&providers(&[1]), 0, 0, &mut visits),
            Some(vec![])
        );
    }

    #[test]
    fn worst_case_pair_table_visits_are_bounded_without_per_pair_allocations() {
        let mut masks = (0u32..u32::from(u16::MAX) + 1)
            .map(|mask| mask as u16)
            .filter(|mask| mask.count_ones() == 5)
            .take(4096)
            .collect::<Vec<_>>();
        // Ensure this 4-cover appears among the bounded distinct-mask set.
        let four_cover = [
            0b0000_0000_0001_1111,
            0b0000_0001_1110_0001,
            0b0001_1110_0000_0001,
            0b1110_0000_0000_0011,
        ];
        for mask in four_cover {
            if !masks.contains(&mask) {
                masks.pop();
                masks.push(mask);
            }
        }
        assert_eq!(masks.len(), 4096);
        assert_eq!(
            masks
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            4096
        );
        let providers = providers(&masks);
        let mut visits = 0;
        let started = std::time::Instant::now();
        let result = find_minimum_cover(&providers, u16::MAX, 4, &mut visits);
        let elapsed = started.elapsed();
        println!(
            "cover_microcheck providers=4096 requests=16 pair_visits={visits} table_bytes={} elapsed_ms={:.3}",
            2 * MASK_COUNT * std::mem::size_of::<PairChoice>(),
            elapsed.as_secs_f64() * 1000.0
        );
        assert_eq!(result.as_ref().map(Vec::len), Some(4));
        assert!(visits <= 8_386_560, "pair visits exceeded the hard bound");
        assert_eq!(visits, 8_386_560);
    }

    #[test]
    fn already_selected_unavailable_provider_still_consumes_route_capacity() {
        use crate::store::Skill;

        let retained = Skill {
            id: "route.unavailable".into(),
            description: "Pinned but unavailable".into(),
            tags: vec![],
            active_version: 1,
            enabled: true,
            manifest_version: None,
        };
        let mut candidates = BTreeMap::new();
        candidates.insert(
            retained.id.clone(),
            Candidate {
                skill: &retained,
                manifest: None,
                mask: 0b0001,
                declared: 0b0001,
                available: false,
            },
        );
        let additions = [
            ("a.one", 0b0001),
            ("b.two", 0b0010),
            ("c.three", 0b0100),
            ("d.four", 0b1000),
        ];
        let skills = additions
            .iter()
            .map(|(id, _)| Skill {
                id: (*id).into(),
                description: "Available provider".into(),
                tags: vec![],
                active_version: 1,
                enabled: true,
                manifest_version: None,
            })
            .collect::<Vec<_>>();
        for (skill, (_, mask)) in skills.iter().zip(additions) {
            candidates.insert(
                skill.id.clone(),
                Candidate {
                    skill,
                    manifest: None,
                    mask,
                    declared: mask,
                    available: true,
                },
            );
        }
        let requested = ["cap.one", "cap.two", "cap.three", "cap.four"]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let requested = requested.iter().collect::<Vec<_>>();
        let mut selected = vec!["route.unavailable".to_owned()];
        let report = select(&candidates, &requested, &mut selected, 0);
        assert_eq!(selected.len(), 4);
        assert_eq!(report.full_coverage_feasible, Some(false));
        assert_eq!(report.selected_additional_count, 3);
        assert!(report.greedy_fallback);
    }
}

use moli_layout::LayoutPassOutput;

use crate::document_runtime::DomHandle;

struct LatestLayoutSnapshot {
    document: DomHandle,
    output: Box<LayoutPassOutput<DomHandle>>,
}

/// Single-slot storage for the latest successful owned layout projection.
///
/// This deliberately owns no layout world, Taffy cache, style borrow, paint
/// snapshot, timer, freshness stamp, or invalidation policy. The context-host
/// orchestration decides when to read, publish, or clear the slot.
#[derive(Default)]
pub(super) struct LatestLayoutSnapshotCache {
    latest: Option<LatestLayoutSnapshot>,
}

impl LatestLayoutSnapshotCache {
    pub(super) fn get(&self, document: DomHandle) -> Option<&LayoutPassOutput<DomHandle>> {
        self.latest
            .as_ref()
            .filter(|snapshot| snapshot.document == document)
            .map(|snapshot| snapshot.output.as_ref())
    }

    pub(super) fn publish(&mut self, document: DomHandle, mut output: LayoutPassOutput<DomHandle>) {
        debug_assert!(
            output.paint_snapshot().is_none(),
            "a retained layout output must not own paint resources"
        );
        if output.paint_snapshot().is_some() {
            drop(output.take_paint_snapshot());
        }
        self.latest = Some(LatestLayoutSnapshot {
            document,
            output: Box::new(output),
        });
    }

    pub(super) fn clear(&mut self) {
        self.latest = None;
    }

    #[cfg(test)]
    pub(super) fn observability(
        &self,
    ) -> Option<(DomHandle, moli_layout::LayoutOutputRetentionMetrics, bool)> {
        self.latest.as_ref().map(|snapshot| {
            (
                snapshot.document,
                snapshot.output.retention_metrics(),
                snapshot.output.paint_snapshot().is_some(),
            )
        })
    }
}

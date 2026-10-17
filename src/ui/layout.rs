use ratatui::layout::{Constraint, Layout, Rect};

pub const MIN_W: u16 = 40;
pub const MIN_H: u16 = 12;
pub const WIDE_W: u16 = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelId {
    AiWorkload,
    AiRuntime,
    Cpu,
    Gpu,
    Memory,
    NetDisk,
    Sensors,
    Processes,
}

impl PanelId {
    pub const ALL: [PanelId; 8] = [
        PanelId::AiWorkload,
        PanelId::AiRuntime,
        PanelId::Cpu,
        PanelId::Gpu,
        PanelId::Memory,
        PanelId::NetDisk,
        PanelId::Sensors,
        PanelId::Processes,
    ];
}

impl PanelId {
    pub fn number(self) -> usize {
        PanelId::ALL.iter().position(|p| *p == self).unwrap() + 1
    }

    pub fn config_name(self) -> &'static str {
        crate::config::PANEL_NAMES[self.number() - 1]
    }

    pub fn from_number(n: usize) -> Option<PanelId> {
        PanelId::ALL.get(n.checked_sub(1)?).copied()
    }
}

const ROWS: [(PanelId, PanelId); 4] = [
    (PanelId::AiWorkload, PanelId::AiRuntime),
    (PanelId::Cpu, PanelId::Gpu),
    (PanelId::Memory, PanelId::NetDisk),
    (PanelId::Sensors, PanelId::Processes),
];

fn preferred_height(p: PanelId) -> Constraint {
    match p {
        PanelId::AiWorkload | PanelId::AiRuntime => Constraint::Length(8),
        _ => Constraint::Fill(1),
    }
}

pub fn compute(area: Rect, visible: &[PanelId], zoomed: Option<PanelId>) -> Vec<(PanelId, Rect)> {
    if let Some(z) = zoomed {
        return vec![(z, area)];
    }
    let mut out = Vec::new();
    if area.width >= WIDE_W {
        let rows: Vec<(Option<PanelId>, Option<PanelId>)> = ROWS
            .iter()
            .map(|(a, b)| {
                (
                    visible.contains(a).then_some(*a),
                    visible.contains(b).then_some(*b),
                )
            })
            .filter(|(a, b)| a.is_some() || b.is_some())
            .collect();
        let heights: Vec<Constraint> = rows
            .iter()
            .map(|(a, b)| preferred_height(a.or(*b).unwrap()))
            .collect();
        let row_areas = Layout::vertical(heights).split(area);
        for ((a, b), r) in rows.iter().zip(row_areas.iter()) {
            match (a, b) {
                (Some(a), Some(b)) => {
                    let [l, rr] = Layout::horizontal([
                        Constraint::Percentage(50),
                        Constraint::Percentage(50),
                    ])
                    .areas(*r);
                    out.push((*a, l));
                    out.push((*b, rr));
                }
                (Some(x), None) | (None, Some(x)) => out.push((*x, *r)),
                (None, None) => {}
            }
        }
    } else {
        let order: Vec<PanelId> = PanelId::ALL
            .into_iter()
            .filter(|p| visible.contains(p))
            .collect();
        let heights: Vec<Constraint> = order
            .iter()
            .map(|p| match p {
                PanelId::AiWorkload | PanelId::AiRuntime => Constraint::Length(8),
                PanelId::Processes => Constraint::Min(8),
                _ => Constraint::Min(6),
            })
            .collect();
        let areas = Layout::vertical(heights).split(area);
        out.extend(order.into_iter().zip(areas.iter().copied()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(v: &[(PanelId, Rect)]) -> Vec<PanelId> {
        v.iter().map(|x| x.0).collect()
    }

    #[test]
    fn numbering_and_names() {
        assert_eq!(PanelId::AiWorkload.number(), 1);
        assert_eq!(PanelId::Processes.number(), 8);
        assert_eq!(PanelId::NetDisk.config_name(), "netdisk");
    }

    #[test]
    fn wide_two_columns() {
        let r = compute(Rect::new(0, 0, 160, 48), &PanelId::ALL, None);
        assert_eq!(ids(&r), PanelId::ALL.to_vec());
        let cpu = r.iter().find(|x| x.0 == PanelId::Cpu).unwrap().1;
        let gpu = r.iter().find(|x| x.0 == PanelId::Gpu).unwrap().1;
        assert_eq!(cpu.y, gpu.y);
        assert!(gpu.x > cpu.x);
        for (_, a) in &r {
            assert!(a.right() <= 160 && a.bottom() <= 48);
        }
    }

    #[test]
    fn hidden_partner_takes_full_width() {
        let vis: Vec<_> = PanelId::ALL
            .into_iter()
            .filter(|p| *p != PanelId::Gpu)
            .collect();
        let r = compute(Rect::new(0, 0, 160, 48), &vis, None);
        let cpu = r.iter().find(|x| x.0 == PanelId::Cpu).unwrap().1;
        assert_eq!(cpu.width, 160);
    }

    #[test]
    fn narrow_single_column() {
        let r = compute(Rect::new(0, 0, 80, 60), &PanelId::ALL, None);
        assert!(r.iter().all(|(_, a)| a.width == 80));
    }

    #[test]
    fn zoom_fills_area() {
        let area = Rect::new(0, 0, 100, 30);
        assert_eq!(
            compute(area, &PanelId::ALL, Some(PanelId::Sensors)),
            vec![(PanelId::Sensors, area)]
        );
    }
}

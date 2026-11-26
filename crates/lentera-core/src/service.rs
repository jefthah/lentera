use crate::{Item, Kind, Library, bible};

/// Satu slide yang siap ditampilkan di pratinjau atau proyektor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slide {
    pub item: u64,
    pub kind: Kind,
    pub title: String,
    pub text: String,
    /// Referensi ayat di slide ini untuk layar proyektor, misalnya
    /// "Yohanes 3:16 (TB)". Kosong untuk lagu dan ayat yang judulnya bukan
    /// referensi (lihat [`bible::slide_captions`]).
    pub caption: String,
    /// Nomor slide, mulai dari 0.
    pub page: usize,
    /// Jumlah slide pada materi.
    pub total: usize,
}

/// Posisi slide yang sedang tayang.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub item: u64,
    pub page: usize,
    /// Urutan materi di susunan ibadah, bila materi ditayangkan dari susunan.
    pub service: Option<usize>,
}

/// Hasil [`Library::step`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Moved(Cursor),
    /// Sudah di slide pertama/terakhir, dan materi tidak tayang dari susunan.
    EndOfItem,
    /// Sudah di awal/akhir susunan ibadah.
    EndOfService,
}

impl Library {
    pub fn slide(&self, item: u64, page: usize) -> Option<Slide> {
        let item = self.item(item)?;
        let slides = item.slides();
        let total = slides.len();
        let captions = match item.kind {
            Kind::Verse => bible::slide_captions(&item.title, &slides),
            Kind::Song => Vec::new(),
        };
        let text = slides.into_iter().nth(page)?;
        Some(Slide {
            item: item.id,
            kind: item.kind,
            title: item.title.clone(),
            text,
            caption: captions.into_iter().nth(page).unwrap_or_default(),
            page,
            total,
        })
    }

    /// Letak materi di susunan ibadah yang sedang dipakai. `hint` (misalnya baris susunan yang
    /// sedang dipilih) dipakai bila menunjuk materi ini, karena satu materi
    /// boleh muncul lebih dari sekali; bila tidak, dipakai kemunculan pertama.
    pub fn service_position(&self, item: u64, hint: Option<usize>) -> Option<usize> {
        let order = self.order();
        hint.filter(|index| order.get(*index) == Some(&item))
            .or_else(|| order.iter().position(|id| *id == item))
    }

    /// Slide berikutnya atau sebelumnya. Setelah slide terakhir sebuah materi,
    /// tayangan pindah ke materi berikutnya di susunan ibadah (mundur: ke slide
    /// terakhir materi sebelumnya). Materi tanpa slide dilewati. Yang dipakai
    /// adalah susunan yang sedang dipakai ([`Library::order`]).
    pub fn step(&self, from: Cursor, forward: bool) -> Step {
        let count = |id| self.item(id).map_or(0, |item: &Item| item.slides().len());
        if let Some(page) = shift(from.page, forward).filter(|page| *page < count(from.item)) {
            return Step::Moved(Cursor { page, ..from });
        }
        let Some(mut position) = from.service else {
            return Step::EndOfItem;
        };
        let order = self.order();
        while let Some(next) = shift(position, forward).filter(|next| *next < order.len()) {
            let id = order[next];
            let total = count(id);
            if total > 0 {
                return Step::Moved(Cursor {
                    item: id,
                    page: if forward { 0 } else { total - 1 },
                    service: Some(next),
                });
            }
            position = next;
        }
        Step::EndOfService
    }
}

fn shift(value: usize, forward: bool) -> Option<usize> {
    if forward {
        value.checked_add(1)
    } else {
        value.checked_sub(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Kind;

    fn at(item: u64, page: usize, service: Option<usize>) -> Cursor {
        Cursor {
            item,
            page,
            service,
        }
    }

    /// Pustaka contoh: materi 1 dan 2 masing-masing dua slide; di antaranya
    /// ada materi kosong di susunan.
    fn library() -> Library {
        let mut library = Library::default();
        let empty = library.add(Kind::Song, "Kosong".into(), " \n\n".into());
        library.service = vec![1, empty, 2];
        library
    }

    #[test]
    fn steps_through_pages_then_service_skipping_empty_items() {
        let library = library();
        assert_eq!(
            library.step(at(1, 0, Some(0)), true),
            Step::Moved(at(1, 1, Some(0)))
        );
        assert_eq!(
            library.step(at(1, 1, Some(0)), true),
            Step::Moved(at(2, 0, Some(2)))
        );
        assert_eq!(
            library.step(at(2, 0, Some(2)), false),
            Step::Moved(at(1, 1, Some(0)))
        );
        assert_eq!(library.step(at(2, 1, Some(2)), true), Step::EndOfService);
        assert_eq!(library.step(at(1, 0, Some(0)), false), Step::EndOfService);
    }

    #[test]
    fn item_outside_service_stops_at_its_own_edges() {
        let library = library();
        assert_eq!(library.step(at(1, 1, None), true), Step::EndOfItem);
        assert_eq!(library.step(at(1, 0, None), false), Step::EndOfItem);
        assert_eq!(
            library.step(at(1, 1, None), false),
            Step::Moved(at(1, 0, None))
        );
    }

    #[test]
    fn slide_reports_text_page_and_total() {
        let library = library();
        let slide = library.slide(1, 1).unwrap();
        assert_eq!(
            slide.text,
            "Kasih Tuhan menyertai kita\nHari ini dan selamanya"
        );
        assert_eq!((slide.item, slide.page, slide.total), (1, 1, 2));
        assert_eq!((slide.kind, slide.caption.as_str()), (Kind::Song, ""));
        assert!(library.slide(1, 2).is_none());
        assert!(library.slide(99, 0).is_none());
    }

    #[test]
    fn verse_slide_carries_its_reference() {
        let mut library = Library::blank();
        let id = library.add(
            Kind::Verse,
            "Yohanes 3:16-17 (TB)".into(),
            "¹⁶Karena begitu besar\n\n¹⁷Sebab Allah mengutus".into(),
        );
        let slide = library.slide(id, 1).unwrap();
        assert_eq!(slide.kind, Kind::Verse);
        assert_eq!(slide.caption, "Yohanes 3:17 (TB)");
        // Judul yang bukan referensi tidak diberi keterangan.
        let sample = library.add(Kind::Verse, "Bacaan pagi".into(), "Teks".into());
        assert_eq!(library.slide(sample, 0).unwrap().caption, "");
    }

    #[test]
    fn service_position_prefers_the_selected_row() {
        let library = Library {
            service: vec![1, 2, 1],
            ..Library::default()
        };
        assert_eq!(library.service_position(1, Some(2)), Some(2));
        assert_eq!(library.service_position(1, Some(1)), Some(0));
        assert_eq!(library.service_position(2, None), Some(1));
        assert_eq!(library.service_position(3, None), None);
    }
}

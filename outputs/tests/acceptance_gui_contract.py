"""Shared named-tab contract for headless and live GUI acceptance."""
EXPECTED_TABS = (
    'Genel durum', 'Güç ve işlemci', 'Fanlar', 'Serin çalışma',
    'Dynamic Boost', 'GPU neden açık?', 'Klavye', 'Profiller', 'Pil',
    'Ekran', 'Otomasyon', 'Cihazlar', 'Tanılama', 'Ölçüm geçmişi',
    'Karşılaştırma', 'SSD ve RAM', 'Kaynaklar', 'Ses', 'Uyku ve pil',
)


def verify_gui_tabs(window):
    names = tuple(window.tabs.tabText(i) for i in range(window.tabs.count()))
    assert names == EXPECTED_TABS, f'GUI tabs differ: {names!r}'
    return names

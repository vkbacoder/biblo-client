// "What's new" notes bundled with the app (ticket 145), keyed by app version then
// locale ("en" is the fallback). Shown once, the first launch after an update to a
// version listed here — works offline and in personal mode. Add an entry when
// cutting a release; versions without an entry show nothing.
export const WHATS_NEW: Record<string, Record<string, string[]>> = {
  "1.6.0": {
    en: [
      "See reports and screenshots for everyone you track right in the desktop app — no web dashboard needed",
      "Fixed only the latest 60 screenshots being viewable — you can now browse them all",
      "Fixed the web dashboard appearing blank or broken in Firefox and with ad blockers on",
      "Fixed some admin screens showing placeholder values instead of real data",
    ],
    vi: [
      "Xem báo cáo và ảnh chụp màn hình của mọi người bạn theo dõi ngay trong ứng dụng — không cần mở bảng điều khiển web",
      "Sửa lỗi chỉ xem được 60 ảnh chụp gần nhất — giờ bạn có thể xem tất cả",
      "Sửa lỗi bảng điều khiển web bị trống hoặc vỡ bố cục trên Firefox và khi bật trình chặn quảng cáo",
      "Sửa lỗi một số màn hình quản trị hiển thị giá trị giữ chỗ thay vì dữ liệu thật",
    ],
    zh: [
      "直接在桌面应用中查看所有被跟踪成员的报告和截图，无需打开网页仪表板",
      "修复了只能查看最近 60 张截图的问题，现在可以浏览全部截图",
      "修复了网页仪表板在 Firefox 中或开启广告拦截时显示空白或布局错乱的问题",
      "修复了部分管理页面显示占位内容而非真实数据的问题",
    ],
    ja: [
      "追跡している全員のレポートとスクリーンショットを、Web ダッシュボードを開かずにデスクトップアプリで直接確認できます",
      "最新 60 枚のスクリーンショットしか表示できなかった問題を修正し、すべて閲覧できるようになりました",
      "Firefox や広告ブロッカー有効時に Web ダッシュボードが空白になる・レイアウトが崩れる問題を修正しました",
      "一部の管理画面で実データの代わりに仮の値が表示される問題を修正しました",
    ],
    id: [
      "Lihat laporan dan tangkapan layar semua orang yang Anda pantau langsung di aplikasi desktop — tanpa membuka dasbor web",
      "Memperbaiki hanya 60 tangkapan layar terbaru yang bisa dilihat — kini Anda bisa melihat semuanya",
      "Memperbaiki dasbor web yang kosong atau berantakan di Firefox dan saat pemblokir iklan aktif",
      "Memperbaiki beberapa layar admin yang menampilkan nilai sementara, bukan data sebenarnya",
    ],
    fr: [
      "Consultez les rapports et captures d'écran de toutes les personnes suivies directement dans l'application, sans ouvrir le tableau de bord web",
      "Correction : seules les 60 dernières captures étaient visibles — vous pouvez désormais toutes les parcourir",
      "Correction du tableau de bord web vide ou mal affiché sous Firefox et avec un bloqueur de publicités",
      "Correction de certains écrans d'administration affichant des valeurs fictives au lieu des vraies données",
    ],
    es: [
      "Consulta los informes y capturas de todas las personas que sigues directamente en la app de escritorio, sin abrir el panel web",
      "Corregido que solo se pudieran ver las últimas 60 capturas: ahora puedes verlas todas",
      "Corregido el panel web que aparecía en blanco o descolocado en Firefox y con bloqueadores de anuncios",
      "Corregidas algunas pantallas de administración que mostraban valores de ejemplo en lugar de datos reales",
    ],
  },
  "1.5.1": {
    en: [
      "Screenshot privacy mode: capture only the active window, and skip the apps you choose.",
      "Dashboard fixes and a new full-screen screenshot viewer.",
    ],
    vi: [
      "Chế độ riêng tư cho ảnh chụp: chỉ chụp cửa sổ đang dùng và bỏ qua các ứng dụng bạn chọn.",
      "Sửa lỗi bảng điều khiển và trình xem ảnh chụp toàn màn hình mới.",
    ],
    zh: [
      "截图隐私模式：只截取当前窗口，并跳过你选择的应用。",
      "仪表盘修复，以及全新的全屏截图查看器。",
    ],
    ja: [
      "スクリーンショットのプライバシーモード：アクティブなウィンドウのみを撮影し、選択したアプリはスキップします。",
      "ダッシュボードの修正と、新しい全画面スクリーンショットビューア。",
    ],
    id: [
      "Mode privasi tangkapan layar: hanya jendela aktif yang ditangkap, dan aplikasi pilihan Anda dilewati.",
      "Perbaikan dasbor dan penampil tangkapan layar layar penuh yang baru.",
    ],
    fr: [
      "Mode confidentialité des captures : seule la fenêtre active est capturée, et les applications choisies sont ignorées.",
      "Corrections du tableau de bord et nouvelle visionneuse de captures en plein écran.",
    ],
    es: [
      "Modo de privacidad de capturas: solo se captura la ventana activa y se omiten las aplicaciones que elijas.",
      "Correcciones del panel y un nuevo visor de capturas a pantalla completa.",
    ],
  },
};

export function whatsNewFor(version: string, locale: string): string[] | null {
  const entry = WHATS_NEW[version];
  if (!entry) return null;
  return entry[locale] ?? entry.en ?? null;
}

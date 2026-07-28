# Codex Usage Monitor

**Languages:** [English (default)](../../README.md) · [한국어](README.ko.md) · [Español](README.es.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [日本語](README.ja.md) · [हिन्दी](README.hi.md) · [Deutsch](README.de.md) · [Français](README.fr.md) · [Tiếng Việt](README.vi.md) · [Türkçe](README.tr.md) · [العربية](README.ar.md)

Codex Usage Monitor, Codex kullanımınızı hızlıca kontrol etmek için hazırlanmış küçük bir yerel Windows aracıdır.
Birincil ve ikincil hız sınırı pencerelerini görev çubuğunda, yüzen bir araçta ve sistem tepsisinde gösterir.

![Codex Usage Monitor görev çubuğu aracı](../images/taskbar-widget-en.png)

## Öne çıkanlar

- Birincil ve ikincil Codex kullanım pencerelerini, sıfırlanma zamanlarıyla birlikte gösterir.
- Kimlik doğrulama dosyalarını ayrıştırmak yerine yüklü Codex CLI'nin `app-server` arayüzünü kullanır.
- Aracı her görev çubuğunda veya yalnızca birincil monitörde göstermeyi destekler.
- Görev çubuğuna ekleme kullanılamadığında güvenli şekilde yüzen araca ve tepsi simgesine geri döner.
- Elle yenileme, otomatik yenileme aralıkları, Windows başlangıcı, tanılama ve yerelleştirilmiş kullanıcı arayüzünü destekler.

## Nasıl çalışır

İzleyici, `codex app-server --stdio` komutunu yerel bir alt süreç olarak başlatır ve standart giriş/çıkış üzerinden JSONL iletileri alışverişi yapar.
Yüklü Codex CLI kendi kimlik doğrulamasını yönetir ve mevcut yapılandırması ile ağ ilkesi kapsamında OpenAI ile iletişim kurabilir.

İzleyici yalnızca görüntüleme için gereken oturum durumunu ve kullanım pencerelerini ister.
Bir Codex görevi başlatmaz veya `codex exec` çağırmaz.

## Gereksinimler

- Windows 10 veya Windows 11, x64.
- `account/read` ve `account/rateLimits/read` desteği olan, oturum açılmış bir [Codex CLI](https://github.com/openai/codex).

## İndirme ve çalıştırma

Önce Codex CLI'nin yüklü olduğunu ve oturumun açık olduğunu doğrulayın:

```powershell
codex --version
codex login status
```

### Kurulum uygulaması (önerilir)

1. `CodexPeek-Setup-v<version>-x64.exe` dosyasını
   [en son GitHub Release](https://github.com/lch5518/CodexPeek/releases/latest) sayfasından indirin.
2. Kurulumu çalıştırın ve yönergeleri izleyin. Yönetici erişimi gerekmez.
3. Başlat Menüsü'nden **Codex Usage Monitor** uygulamasını başlatın.

### Taşınabilir sürüm

1. `codex-peek-v<version>-windows-x86_64-portable.zip` dosyasını en son sürümden indirin.
2. ZIP dosyasını tamamen yazılabilir bir klasöre çıkarın.
3. Çıkardığınız klasörden `codex-peek.exe` dosyasını çalıştırın.

### Kaynaktan derleme

Bu seçenek Rust 1.85 veya üzerini, Visual Studio 2022 C++ Build Tools'u ve bir
Windows SDK'yı gerektirir. Uygulamayı klonlanmış depodan çalıştırır ve Başlat
Menüsü kısayolu veya kaldırıcı oluşturmaz.

```powershell
git clone https://github.com/lch5518/CodexPeek.git
Set-Location .\CodexPeek
cargo build --release
.\target\release\codex-peek.exe
```

Kullanıcı arayüzünü açmadan derlemeyi ve Codex CLI bağlantısını kontrol etmek için:

```powershell
.\target\release\codex-peek.exe --diagnose
```

### Codex'ten kurmasını isteyin

Aşağıdaki istemi Codex'e kopyalayın. Doğrulanmış Kurulum uygulamasını tercih eder ve
yalnızca uyumlu Release varlıkları yoksa kaynak derlemeye geri döner.

```text
Bu Windows x64 bilgisayara CodexPeek'i kur ve doğrulamayı benim için tamamla.

1. Bunun Windows x64 olduğunu doğrula, ardından `codex --version` ve `codex login status` komutlarını çalıştır.
2. Yalnızca resmi depoyu ve onun Releases varlıklarını kullan:
   https://github.com/lch5518/CodexPeek
3. En son `CodexPeek-Setup-v<version>-x64.exe` dosyasını tercih et. Onu
   `SHA256SUMS.txt` ile birlikte indir, bu dosyada tam Installer girdisini bul,
   Installer'ın SHA-256 değerini hesapla ve yalnızca karmalar eşleşirse devam et.
   Güvenlik denetimlerini devre dışı bırakma veya sağlama toplamı eksik ya da farklı
   olan bir dosyayı çalıştırma.
4. Yönetici erişimi istemeden geçerli kullanıcı için kur. Mevcut CodexPeek
   ayarlarını koru ve çalışan uygulamayı ya da ilgisiz bir süreci durdurma; uygulamayı
   kendim kapatmam gerekiyorsa bana söyle.
5. Yalnızca uyumlu Release varlıkları yoksa resmi depoyu kullanıcı tarafından
   yazılabilir yeni bir dizine klonla ve `cargo build --release` çalıştır. Git, Rust 1.85+,
   Visual Studio 2022 C++ Build Tools veya bir Windows SDK kurulması gerekiyorsa,
   önce tam olarak neyin değişeceğini açıkla ve onayımı iste.
6. `%USERPROFILE%\.codex\auth.json` içeriğini asla okuma veya yazdırma. Kimlik
   doğrulama yalnızca yüklü Codex CLI üzerinden yönetilmelidir.
7. Kurulum veya derlemeden sonra ortaya çıkan `codex-peek.exe --diagnose` komutunu çalıştır.
   Başarılı olursa CodexPeek'i başlat.
8. Seçilen kurulum yöntemini, kurulu sürümü, yürütülebilir dosyanın konumunu, sağlama
   toplamı sonucunu ve tanılama sonucunu raporla. Herhangi bir şey başarısız olursa,
   güvenli şekilde dur ve hassas bilgileri açığa çıkarmadan tam engeli açıkla.
```

Kurulum ve Taşınabilir sürümler `%APPDATA%\CodexUsageMonitor\settings.json` dosyasını kullanır; bu nedenle
bu sürümler arasında geçiş yaparsanız ayarlar paylaşılır. Kurulum uygulaması Başlat Menüsü kısayolu ekler
ancak Windows başlangıcını varsayılan olarak etkinleştirmez.

İlk sürümler kod imzalı değildir ve Microsoft Defender SmartScreen uyarısını tetikleyebilir.
Yalnızca resmi sürümden indirin ve dosyayı `SHA256SUMS.txt` ile doğrulayın.

Karma doğrulama, güncellemeler, kaldırma davranışı, tanılama ve sorun giderme için
[ayrıntılı kurulum kılavuzuna (Korece)](../INSTALL.md) bakın.

## İzleyiciyi kullanma

Kullanımı yenilemek, 1/5/10/15/30 dakikalık yenileme aralığı seçmek ve aracı göstermek veya gizlemek için tepsi menüsünü kullanın.
Menüde ayrıca Windows başlangıcı, başlangıç görünümü, kimlik doğrulamayı yenileme, otomatik kimlik doğrulama yenileme, dil ve tanılama ayarları bulunur.
Çoklu monitör yerleşimini kontrol etmek için **Widget: all monitors** veya **Widget: primary monitor only** seçeneğini seçin; seçim yeniden başlatmalar arasında hatırlanır.

Varsayılan olarak kullanıcı arayüzü dili, desteklenen bir dille eşleştiğinde Windows yerel ayarını izler. Tepsi menüsünden dili elle de seçebilirsiniz. Desteklenen diller Korece, İngilizce, İspanyolca, Brezilya Portekizcesi, Endonezce, Japonca, Hintçe, Almanca, Fransızca, Vietnamca, Türkçe ve Arapçadır.

Görev çubuğu aracı, metni için Windows açık/koyu sistem temasını kullanır ve yerel görev çubuğu malzemesinin arka plandan görünmesine izin verir.

Aynı anda yalnızca bir kullanım isteği çalışır. Başarısız istekler artan gecikmelerle yeniden denenirken son başarılı değerler görünür kalır.

Explorer yeniden başlatıldıktan veya görev çubuğu yerleşimi değiştikten sonra görev çubuğu aracı eklenemezse, tepsi simgesi kullanılabilir kalır ve izleyici güvenli şekilde yeniden dener.

## Gizlilik ve güvenlik

İzleyici `%USERPROFILE%\.codex\auth.json` içeriğini asla okumaz veya ayrıştırmaz.
Tanılama yalnızca bu yolun var olup olmadığını kontrol eder.

Ham RPC yanıtları yalnızca oturum açma türünü ve görüntülenen hız sınırı alanlarını çıkarmaya yetecek kadar işlenir.
Token'lar, hesap ID'leri, e-posta adresleri, kimlik doğrulama dosyası içerikleri ve proxy değerleri saklanmaz veya günlüklere yazılmaz.

Ayarlar `%APPDATA%\CodexUsageMonitor\settings.json` içinde saklanır.
Sınırlı tanılama günlüğü `%TEMP%\codex-peek.log` içinde saklanır.

Eksiksiz veri işleme ve güvenlik açığı bildirme yönergeleri için [SECURITY.md](../../SECURITY.md) dosyasına bakın.

## Sorun giderme

| Sorun | Ne yapmalı |
| --- | --- |
| Codex CLI bulunamıyor | `codex --version` ve `where.exe codex` komutlarını çalıştırın, ardından Codex CLI'nin `PATH` üzerinde olduğundan emin olun. |
| CLI desteklenmiyor | Codex CLI'yi güncelleyin. Gerekli RPC desteği, görüntülenen sürüm numarasından daha önemlidir. |
| Oturum kapalı veya kimlik doğrulama süresi dolmuş | Codex CLI'de normal oturum açma akışını tamamlayın, ardından tepsi menüsünden **Refresh authentication** seçeneğini seçin. |
| Görev çubuğu aracı yanlış monitörde | Tepsi menüsünden **Widget: all monitors** veya **Widget: primary monitor only** seçeneğini seçin. |
| Görev çubuğu aracı eksik | Yüzen aracı veya tepsi simgesini kullanın, gerekirse Explorer'ı yeniden başlatın ve tercih edilen araç monitörü modunu seçin. |
| Daha fazla ayrıntı gerekiyor | `--diagnose` komutunu çalıştırın veya tepsi menüsünden **Diagnostics** öğesini açın. |

## Geliştirme

Kaynak derlemeler Rust 1.85 veya üzerini, Visual Studio 2022 C++ Build Tools'u ve bir
Windows SDK'yı gerektirir. Depo kökünden derleyin ve doğrulayın:

```powershell
git clone https://github.com/lch5518/CodexPeek.git
Set-Location .\CodexPeek
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo build --release
```

Otomatik kontroller, [sürüm kontrol listesindeki](../RELEASE_CHECKLIST.md) Windows, DPI, çoklu monitör ve Explorer kurtarma senaryolarının yerini tutmaz.

## ❤️ Destek

CodexPeek size zaman kazandırıyorsa geliştirmesini desteklemeyi değerlendirin.

- ⭐ Bu depoya yıldız verin
- ❤️ [GitHub'da Sponsor Olun](https://github.com/sponsors/lch5518)

Her sponsorluk projenin aktif olarak bakımının sürdürülmesine yardımcı olur.

## Lisans

Bu proje [MIT Lisansı](../../LICENSE) kapsamında sunulur.
Üçüncü taraf bildirimleri için [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md) dosyasına bakın.

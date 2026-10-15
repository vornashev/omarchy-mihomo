pragma Singleton
import QtQuick

// UI strings. Russian by default; `locale: "en"` in the widget settings.
// Placeholders use Qt's %1, %2... filled with String.arg().
QtObject {
  id: root

  property string language: "ru"

  function t(key) {
    var table = language === "en" ? en : ru
    var v = table[key]
    if (v === undefined) v = en[key]
    return v === undefined ? key : v
  }

  function kind(k) { return t("kind_" + k) }
  function mode(m) { return m ? t("mode_" + m) : "" }
  function matchKind(k) { return t("match_" + k) }
  function matchKindLong(k) { return t("matchLong_" + k) }
  function ignoreKind(k) { return t("ignore_" + k) }
  function action(a) { return t("action_" + a) }

  readonly property var ru: ({
    navOverview: "Обзор", navProblems: "Проблемы", navRules: "Правила", navProxies: "Прокси",
    navProfiles: "Профили", navConnections: "Соединения", navLogs: "Логи", navSettings: "Настройки",

    daemonDown: "Демон omarchy-mihomo не запущен",
    daemonHint: "systemctl --user start omarchy-mihomod",
    startDaemon: "Запустить",
    noDaemon: "нет демона",
    coreDown: "mihomo: ядро не запущено",
    coreRunning: "Ядро работает",
    coreStopped: "Ядро остановлено",
    coreLine: "mihomo %1 · mixed-port %2",
    barTooltip: "mihomo · %1 · %2\nНовых проблем: %3",
    on: "вкл", off: "выкл",

    mode_rule: "Правила", mode_global: "Глобальный", mode_direct: "Напрямую",
    modeLabel: "Режим",
    tunHint: "Весь трафик системы через ядро",

    statDown: "Загрузка", statUp: "Отдача", statConns: "Соединения", statNew: "Новые проблемы",
    statNewSub: "%1 за 14 дней",
    applyConfig: "Применить конфиг", configApplied: "Конфиг применён",
    updatedAgo: "обновлён %1", update: "Обновить", profileUpdated: "Профиль обновлён",
    usage: "%1 из %2", until: "до %1",
    recentProblems: "НОВЫЕ ПРОБЛЕМЫ", allLink: "все →",

    problemTable: "Таблица", problemGroups: "По маршрутам",
    problemRecovered: "потом восстановилось", keyGroup: "свернуть",
    problemHost: "ХОСТ", problemKind: "ТИП", problemApp: "ПРИЛОЖЕНИЕ", problemRoute: "МАРШРУТ", problemFailures: "СБОЕВ",
    problemOk: "ок", problemOkCount: "ок ×%1", problemAddresses: "адресов: %1",
    groupEditorTitle: "Маршрут для группы", groupScopeHint: "все перечисленные ресурсы", groupSaved: "Маршрут для %1 ресурсов → %2 сохранён",
    groupProbeHint: "Проверяется один ресурс: %1. Правила будут созданы для всех адресов группы.",
    problemsTitle: "Проблемные ресурсы",
    problemsSubtitle: "%1 ресурсов · %2 новых · %3 скрыто",
    searchHint: "Поиск  /",
    markSeen: "Просмотрено",
    tabNew: "Новые", tabAll: "Все", tabHidden: "Скрытые", tabProxy: "Через прокси", tabOff: "Выключены",
    diagnosisHint: "Ошибка соединения — это диагноз, а не блокировка. Маршрут меняется только вручную.",
    noNewProblems: "Новых проблем нет", noProblems: "Проблем нет",
    unknownApp: "приложение неизвестно",
    okSince: "потом ок ×%1",
    since: "с %1",
    route: "Маршрут",
    hiddenToast: "%1 скрыт",
    keyNav: "навигация", keyRoute: "маршрут", keyHide: "скрыть", keySearch: "поиск", keySeen: "просмотрено",
    ignore_host: "хост", ignore_suffix: "домен", ignore_app: "приложение",

    kind_timeout: "таймаут", kind_reset: "сброс соединения", kind_refused: "отказ",
    kind_dns_fail: "DNS не разрешён", kind_dns_stub: "DNS-заглушка", kind_no_route: "нет маршрута",
    kind_tls: "TLS", kind_proxy: "ошибка прокси", kind_other: "другое",

    editorTitle: "Маршрут для ресурса", editRuleTitle: "Изменить правило",
    factErrors: "Ошибки", factApps: "Приложения", factRoute: "Сейчас идёт через", factPeriod: "Период",
    repeats: "%1 повторов",
    scopeLabel: "ОБЛАСТЬ ПРАВИЛА", scopeExact: "только этот домен", scopeSuffix: "домен и все поддомены",
    scopeSuffixHint: "домен из правила, с поддоменами", scopeIp: "только этот адрес",
    targetLabel: "МАРШРУТ", targetHint: "DIRECT или группа из текущего профиля",
    current: "сейчас", ms: "мс", timeout: "таймаут", failed: "ошибка",
    checkOk: "Проверка пройдена: %1 через %2 · %3 мс",
    checkFail: "Проверка не прошла: %1 через %2 — %3.",
    inconclusive: "Для IP и не-веб портов HTTPS-проверка может не проходить, даже если сервис работает.",
    abovePreview: "встанет выше правил подписки",
    replacesExisting: "заменит текущий маршрут → %1",
    manualOnly: "Без автоматики: правило сохранится только по нажатию",
    saveApply: "Сохранить и применить", check: "Проверить", cancel: "Отмена",
    savedToast: "%1 → %2 сохранено", closedConns: "закрыто соединений: %1", notReloaded: "ядро не перезагружено",

    rulesTitle: "Мои правила", rulesSubtitle: "%1 правил · выше правил подписки",
    history: "История", undo: "Откатить", rollbackHere: "Вернуть к этому",
    orphanBanner: "Группа %1 исчезла из профиля — правил пропущено: %2. Трафик идёт по правилам подписки.",
    orphanRow: "группа не найдена в профиле · правило пропущено",
    fix: "Исправить",
    importedFrom: "импорт: %1", added: "добавлено %1", checkedMs: "проверено %1 мс", hits: "%1 совпадений",
    noRules: "Правил пока нет", noRulesHint: "Откройте проблемный ресурс и выберите маршрут",
    deletedToast: "%1 удалено", undoneToast: "Последнее изменение отменено", rolledBack: "Отменено изменений: %1",
    lastChange: "Последнее: %1 %2 · %3", noChanges: "Изменений ещё не было",
    action_add: "добавлено", action_update: "изменено", action_delete: "удалено", action_revert: "отмена",
    match_domain: "DOMAIN", match_domain_suffix: "SUFFIX", "match_domain-suffix": "SUFFIX", "match_ip-cidr": "IP",
    matchLong_domain: "только этот домен", "matchLong_domain-suffix": "домен и все поддомены", "matchLong_ip-cidr": "адрес или сеть",

    proxiesTitle: "Прокси-группы", proxiesSubtitle: "профиль %1 · групп: %2", testAll: "Тест всех",

    profilesTitle: "Профили", profilesSubtitle: "Подписки и локальные конфиги; оверрайды переживают обновление",
    globalOverrides: "Общие оверрайды", updateAll: "Обновить все",
    addPlaceholder: "https://… подписка или путь к YAML", add: "Добавить",
    profileAdded: "Профиль добавлен", profileSelected: "Активный профиль: %1",
    remote: "подписка", local: "локальный", active: "активен", use: "Выбрать",
    overridesHint: "Оверрайды открыты в редакторе; после сохранения нажмите «Применить конфиг»",
    noProfiles: "Профилей нет — добавьте подписку",
    newFile: "Новый файл", newFileName: "Мой конфиг", editFile: "Открыть файл профиля", editOverrides: "Оверрайды",
    newFileHint: "Вставьте конфиг в редакторе и сохраните (Ctrl+S) — профиль подхватится сам",
    tunFailedShort: "TUN не поднялся",
    issue_core_missing: "mihomo не установлен", issue_core_installing: "Скачиваю mihomo…",
    issue_core_install_failed: "Не удалось скачать mihomo: %1",
    issue_other_core: "Работает ядро %1 — два ядра мешают друг другу (TUN и маршруты)",
    issue_core_no_caps: "TUN включён, но у ядра нет прав его открыть",
    issue_core_stopped: "Ядро не запущено", issue_tun_failed: "TUN включён, но ядро не смогло его поднять: %1",
    issue_no_profile: "Профилей пока нет — добавьте подписку или файл",
    issue_autotune: "Таймер site-autotune всё ещё переписывает правила Clash Verge",
    fix_install_core: "Установить", fix_start_core: "Запустить", fix_restart_core: "Перезапустить",
    fix_import_cvr: "Импорт из Clash Verge", fix_disable_autotune: "Отключить", fix_setcap: "Выдать права",
    fix_migrate_cvr: "Перейти с Clash Verge",
    confirm_migrate_cvr: "Импортирую профили Clash Verge, закрою приложение, отключу его сервис и автозапуск, запущу наше ядро. Ничего не удаляется; вернуть: omm migrate cvr --undo",
    confirm_disable_autotune: "Таймер site-autotune будет остановлен и отключён",
    privilegedHint: "Откроется терминал — sudo спросит пароль", confirm: "Подтвердить", fixDone: "Готово",
    draft: "ждёт сохранения", profileSaved: "Профиль «%1» сохранён", profileSaveFailed: "Ошибка в профиле «%1»",

    connectionsTitle: "Соединения", connectionsSubtitle: "активных: %1", filter: "Фильтр", closeAll: "Закрыть все",
    noConnections: "Нет соединений",

    logsTitle: "Логи", logsSubtitle: "Поток журнала ядра", lvlAll: "Все", lvlWarn: "Предупреждения", lvlError: "Ошибки",
    noLogs: "Пока пусто",

    settingsTitle: "Настройки", settingsSubtitle: "Применяются поверх любого профиля",
    allowLan: "Доступ из локальной сети", allowLanHint: "Разрешить другим устройствам подключаться к mixed-port",
    ipv6Hint: "Разрешать IPv6-соединения через ядро",
    processNames: "Имена приложений", processNamesHint: "Определять процесс для каждого соединения (нужно для колонки «приложение»)",
    tunStack: "Стек TUN", mixedPort: "Mixed-port", updateInterval: "Автообновление подписок, ч",
    hours0off: "0 — выключено", enterToSave: "Enter — сохранить", language: "Язык",
    settingsSaved: "Настройки сохранены и применены",
    settingsFootnote: "Контроллер ядра доступен только через unix-сокет, без пароля и TCP. Изменения применяются сразу, с проверкой через mihomo -t.",

    never: "никогда", justNow: "только что", minAgo: "%1 мин назад", hAgo: "%1 ч назад", dAgo: "%1 дн назад",
    unitB: "Б", unitKB: "КБ", unitMB: "МБ", unitGB: "ГБ", unitTB: "ТБ", perSec: "/с"
  })

  readonly property var en: ({
    navOverview: "Overview", navProblems: "Problems", navRules: "Rules", navProxies: "Proxies",
    navProfiles: "Profiles", navConnections: "Connections", navLogs: "Logs", navSettings: "Settings",

    daemonDown: "omarchy-mihomo daemon is not running",
    daemonHint: "systemctl --user start omarchy-mihomod",
    startDaemon: "Start",
    noDaemon: "no daemon",
    coreDown: "mihomo: core is not running",
    coreRunning: "Core running",
    coreStopped: "Core stopped",
    coreLine: "mihomo %1 · mixed-port %2",
    barTooltip: "mihomo · %1 · %2\nNew problems: %3",
    on: "on", off: "off",

    mode_rule: "Rules", mode_global: "Global", mode_direct: "Direct",
    modeLabel: "Mode",
    tunHint: "Route all system traffic through the core",

    statDown: "Download", statUp: "Upload", statConns: "Connections", statNew: "New problems",
    statNewSub: "%1 in 14 days",
    applyConfig: "Apply config", configApplied: "Config applied",
    updatedAgo: "updated %1", update: "Update", profileUpdated: "Profile updated",
    usage: "%1 of %2", until: "until %1",
    recentProblems: "NEW PROBLEMS", allLink: "all →",

    problemTable: "Table", problemGroups: "By route",
    problemRecovered: "recovered afterwards", keyGroup: "collapse",
    problemHost: "HOST", problemKind: "TYPE", problemApp: "APP", problemRoute: "ROUTE", problemFailures: "FAILS",
    problemOk: "ok", problemOkCount: "ok ×%1", problemAddresses: "addresses: %1",
    groupEditorTitle: "Route for group", groupScopeHint: "all listed resources", groupSaved: "Route for %1 resources → %2 saved",
    groupProbeHint: "Testing one resource: %1. Rules will be created for every address in the group.",
    problemsTitle: "Problem resources",
    problemsSubtitle: "%1 resources · %2 new · %3 hidden",
    searchHint: "Search  /",
    markSeen: "Mark seen",
    tabNew: "New", tabAll: "All", tabHidden: "Hidden", tabProxy: "Proxied", tabOff: "Off",
    diagnosisHint: "A connection error is a diagnosis, not a block. Routes change only when you decide.",
    noNewProblems: "No new problems", noProblems: "No problems",
    unknownApp: "unknown app",
    okSince: "then ok ×%1",
    since: "since %1",
    route: "Route",
    hiddenToast: "%1 hidden",
    keyNav: "navigate", keyRoute: "route", keyHide: "hide", keySearch: "search", keySeen: "mark seen",
    ignore_host: "host", ignore_suffix: "domain", ignore_app: "app",

    kind_timeout: "timeout", kind_reset: "reset", kind_refused: "refused",
    kind_dns_fail: "DNS failed", kind_dns_stub: "DNS sinkhole", kind_no_route: "no route",
    kind_tls: "TLS", kind_proxy: "proxy error", kind_other: "other",

    editorTitle: "Route a resource", editRuleTitle: "Edit rule",
    factErrors: "Errors", factApps: "Apps", factRoute: "Currently via", factPeriod: "Period",
    repeats: "%1 repeats",
    scopeLabel: "RULE SCOPE", scopeExact: "this host only", scopeSuffix: "domain and all subdomains",
    scopeSuffixHint: "domain from the matched rule, with subdomains", scopeIp: "this address only",
    targetLabel: "ROUTE", targetHint: "DIRECT or a group of the current profile",
    current: "current", ms: "ms", timeout: "timeout", failed: "failed",
    checkOk: "Check passed: %1 via %2 · %3 ms",
    checkFail: "Check failed: %1 via %2 — %3.",
    inconclusive: "For IPs and non-web ports the HTTPS check may fail even when the service works.",
    abovePreview: "goes above the subscription's rules",
    replacesExisting: "replaces the current route → %1",
    manualOnly: "Nothing automatic: the rule is saved only when you press Save",
    saveApply: "Save and apply", check: "Check", cancel: "Cancel",
    savedToast: "%1 → %2 saved", closedConns: "closed %1 connection(s)", notReloaded: "core not reloaded",

    rulesTitle: "My rules", rulesSubtitle: "%1 rules · above the subscription's rules",
    history: "History", undo: "Undo", rollbackHere: "Roll back to here",
    orphanBanner: "Group %1 is gone from the profile — %2 rule(s) skipped. Traffic follows the subscription's rules.",
    orphanRow: "group not found in the profile · rule skipped",
    fix: "Fix",
    importedFrom: "imported: %1", added: "added %1", checkedMs: "checked %1 ms", hits: "%1 hits",
    noRules: "No rules yet", noRulesHint: "Open a problem resource and choose a route",
    deletedToast: "%1 deleted", undoneToast: "Last change undone", rolledBack: "Undid %1 change(s)",
    lastChange: "Last: %1 %2 · %3", noChanges: "No changes yet",
    action_add: "added", action_update: "changed", action_delete: "deleted", action_revert: "undo",
    match_domain: "DOMAIN", "match_domain-suffix": "SUFFIX", "match_ip-cidr": "IP",
    matchLong_domain: "this host only", "matchLong_domain-suffix": "domain and all subdomains", "matchLong_ip-cidr": "address or network",

    proxiesTitle: "Proxy groups", proxiesSubtitle: "profile %1 · %2 groups", testAll: "Test all",

    profilesTitle: "Profiles", profilesSubtitle: "Subscriptions and local configs; overrides survive updates",
    globalOverrides: "Global overrides", updateAll: "Update all",
    addPlaceholder: "https://… subscription or path to YAML", add: "Add",
    profileAdded: "Profile added", profileSelected: "Active profile: %1",
    remote: "subscription", local: "local", active: "active", use: "Use",
    overridesHint: "Overrides opened in the editor; press Apply config after saving",
    noProfiles: "No profiles — add a subscription",
    newFile: "New file", newFileName: "My config", editFile: "Open profile file", editOverrides: "Overrides",
    newFileHint: "Paste a config in the editor and save (Ctrl+S) — the profile is picked up automatically",
    tunFailedShort: "TUN failed",
    issue_core_missing: "mihomo is not installed", issue_core_installing: "Downloading mihomo…",
    issue_core_install_failed: "Downloading mihomo failed: %1",
    issue_other_core: "%1 runs its own core — two cores fight over TUN and routes",
    issue_core_no_caps: "TUN is on, but the core lacks the rights to open it",
    issue_core_stopped: "The core is not running", issue_tun_failed: "TUN is on, but the core could not start it: %1",
    issue_no_profile: "No profiles yet — add a subscription or a file",
    issue_autotune: "The site-autotune timer still rewrites Clash Verge's rules",
    fix_install_core: "Install", fix_start_core: "Start", fix_restart_core: "Restart",
    fix_import_cvr: "Import from Clash Verge", fix_disable_autotune: "Disable", fix_setcap: "Grant rights",
    fix_migrate_cvr: "Switch from Clash Verge",
    confirm_migrate_cvr: "Imports Clash Verge's profiles, closes the app, switches off its service and autostart, starts our core. Nothing is deleted; undo: omm migrate cvr --undo",
    confirm_disable_autotune: "The site-autotune timer will be stopped and disabled",
    privilegedHint: "Opens a terminal — sudo will ask for the password", confirm: "Confirm", fixDone: "Done",
    draft: "waiting for save", profileSaved: "Profile “%1” saved", profileSaveFailed: "Error in profile “%1”",

    connectionsTitle: "Connections", connectionsSubtitle: "%1 active", filter: "Filter", closeAll: "Close all",
    noConnections: "No connections",

    logsTitle: "Logs", logsSubtitle: "Core log stream", lvlAll: "All", lvlWarn: "Warnings", lvlError: "Errors",
    noLogs: "Nothing yet",

    settingsTitle: "Settings", settingsSubtitle: "Applied on top of any profile",
    allowLan: "Allow LAN", allowLanHint: "Let other devices use the mixed port",
    ipv6Hint: "Allow IPv6 connections through the core",
    processNames: "App names", processNamesHint: "Look up the process of every connection (needed for the app column)",
    tunStack: "TUN stack", mixedPort: "Mixed port", updateInterval: "Subscription refresh, h",
    hours0off: "0 = off", enterToSave: "Enter to save", language: "Language",
    settingsSaved: "Settings saved and applied",
    settingsFootnote: "The core's controller is reachable only through a unix socket, without a password or TCP. Changes apply at once, validated with mihomo -t.",

    never: "never", justNow: "just now", minAgo: "%1 min ago", hAgo: "%1 h ago", dAgo: "%1 d ago",
    unitB: "B", unitKB: "KB", unitMB: "MB", unitGB: "GB", unitTB: "TB", perSec: "/s"
  })
}

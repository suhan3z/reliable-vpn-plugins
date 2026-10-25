#!/usr/bin/env python3
"""Build the reviewed, bounded Russian service catalog without network or keys."""
import argparse
import collections
import json
import pathlib

from validate import validate

ROOT = pathlib.Path(__file__).resolve().parent
CATEGORIES = [
    ('banks', 'Банки, платежи и страхование', True),
    ('government', 'Госуслуги', True),
    ('shopping', 'Покупки, услуги и поездки', False),
    ('media', 'Медиа', False),
    ('ecosystems', 'Связь, социальные сети и сервисы', False),
]
# Whole hosting/CDN providers and foreign services must not become direct rules.
FORBIDDEN = {
    'ru', 'su', 'xn--p1ai', 'gov.ru', 'spb.ru', 'msk.ru', 'com.ru',
    'cloudflare.com', 'cloudfront.net', 'amazonaws.com', 'akamaized.net',
    'cdn77.org', 'fastly.net', 'ngenix.net', 'cdnvideo.ru', 'clstorage.net',
    'yandex.net', 'yandexcloud.net', 'yastatic.net', 'storage.yandexcloud.net',
    'selectel.ru', 'tilda.ws', 'ucoz.ru', 'github.io', 'bitrix24.ru',
    'google.com', 'youtube.com', 'telegram.org', 't.me', 'chatgpt.com',
    'openai.com', 'discord.com', 'facebook.com', 'instagram.com',
}


def ascii_domain(value):
    return value.encode('idna').decode('ascii').lower()


def read_services(directory=ROOT):
    rows = []
    for line in (directory / 'top200.tsv').read_text(encoding='utf-8').splitlines():
        if not line.strip() or line.startswith('#'):
            continue
        category, service, primary, aliases = line.split('|')
        rows.append((category, service, ascii_domain(primary),
                     [ascii_domain(d) for d in aliases.split()]))
    assert len(rows) == 200, 'Exactly 200 reviewed primary sites are required'
    assert len({r[2] for r in rows}) == 200, 'Duplicate primary site'
    assert len({r[1] for r in rows}) == 200, 'Duplicate service name'
    return rows


def build(directory=ROOT):
    metadata = json.loads((directory / 'top200.sources.json').read_text(encoding='utf-8'))
    entries = {}
    used_dependencies = set()
    for category, service, primary, aliases in read_services(directory):
        for domain in [primary] + aliases:
            assert domain not in entries, f'Domain assigned twice: {domain}'
            assert domain not in FORBIDDEN, f'Overbroad or foreign rule: {domain}'
            if domain != primary:
                evidence = metadata['dependencies'][domain]
                used_dependencies.add(domain)
                reason = 'Дополнительный адрес сервиса; источник принадлежности указан отдельно. Совместимость с VPN не проверена.'
            else:
                evidence = {'source': f'https://{primary}/',
                            'checked_at': metadata['checked_at'], 'subdomains': True}
                reason = 'Популярный российский сервис: прямой доступ при включенной категории. Блокировка VPN не подтверждена.'
                if domain in metadata['community_foreign_access_reports']:
                    reason = 'Есть сообщение сообщества об ограничении зарубежного доступа; проверка через VPN из российских сетей не выполнена.'
                if domain in metadata['vpn_guidance']:
                    evidence = dict(evidence, source=metadata['vpn_guidance'][domain]['source'])
                    reason = metadata['vpn_guidance'][domain]['reason']
            entries[domain] = dict(domain=domain, subdomains=evidence['subdomains'],
                                   service=service, category=category,
                                   source=evidence['source'], reason=reason,
                                   checked_at=evidence['checked_at'])
    assert used_dependencies == set(metadata['dependencies']), 'Unused dependency evidence'
    return validate(dict(version=1, generation=metadata['generation'], expires_at=0,
                         updated_at=metadata['checked_at'],
                         categories=[dict(id=i, title=t, default_enabled=d) for i, t, d in CATEGORIES],
                         zones=['ru', 'xn--p1ai', 'su'], entries=list(entries.values())))


def report(directory=ROOT):
    catalog = build(directory)
    metadata = json.loads((directory / 'top200.sources.json').read_text(encoding='utf-8'))
    lines = [
        '# 200 популярных российских сайтов для прямого доступа', '',
        f'Обновлено {catalog["updated_at"]}; поколение {catalog["generation"]}. '
        f'200 основных сайтов, {len(catalog["entries"])} доменных правил вместе с адресами входа, API, оплаты и содержимого.', '',
        'Это отобранный набор популярных и повседневных сервисов, а не измеренный рейтинг посещаемости '
        'и не список 200 доказанных блокировок VPN. При отборе учтены '
        '[Semrush](https://www.semrush.com/trending-websites/ru/all) и '
        '[Similarweb](https://www.similarweb.com/top-websites/russian-federation/), '
        'значимость платежей, государственных и бытовых услуг. Порядок строк тематический.', '',
        'Существующий выбор категорий, личные правила и выключенные зоны сохранены. '
        'По умолчанию по-прежнему включены только банки и госуслуги. '
        'Отключение категории не отменяет другое включенное правило для родительского домена или зоны. '
        'Личное правило «Через VPN» имеет приоритет.', '',
        'Статусы: **справка** — официальный совет отключить VPN при конкретной проблеме; '
        '**сообщество** — сообщение об ограничении зарубежного доступа; '
        '**совместимость** — профилактическое исключение без доказанной блокировки. '
        'Проверка HTTPS-источника не равна проверке входа, оплаты или маршрута через VPN. '
        'Даже прямой IP-маршрут не гарантирует устранение проверки наличия VPN самим приложением.', '',
        '| Категория | Основных сайтов |', '|---|---:|',
    ]
    counts = collections.Counter(r[0] for r in read_services(directory))
    lines += [f'| {title} | {counts[key]} |' for key, title, _ in CATEGORIES]
    lines += ['', '| № | Сервис | Основной сайт | Дополнительные домены | Основание |',
              '|---:|---|---|---|---|']
    for index, (_, service, primary, aliases) in enumerate(read_services(directory), 1):
        guidance = metadata['vpn_guidance'].get(primary)
        if guidance:
            status = f'[справка]({guidance["source"]})'
        elif primary in metadata['community_foreign_access_reports']:
            status = f'[сообщество]({metadata["community_foreign_access_source"]})'
        else:
            status = 'совместимость'
        lines.append(f'| {index} | {service} | [{primary}](https://{primary}/) | '
                     f'{", ".join(aliases) or "—"} | {status} |')
    lines += ['', 'Источники дополнительных доменов и даты: [top200.sources.json](top200.sources.json). '
              'Данные сообщества закреплены по коммиту, а не по изменяемой ветке. '
              'Общие хостинги/CDN, целые доменные зоны и сайты, которым нужен зарубежный VPN, не импортировались. '
              'Присутствуют только конкретные зависимости сервисов; например, '
              '`kinopoisk-ru.clstorage.net`, а не весь `clstorage.net`.', '']
    return '\n'.join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    source = json.dumps(build(), ensure_ascii=False, indent=2) + '\n'
    outputs = {'catalog.source.json': source, 'TOP200.md': report()}
    for filename, content in outputs.items():
        path = ROOT / filename
        if args.check:
            assert path.read_text(encoding='utf-8') == content, f'Regenerate {filename}'
        else:
            path.write_text(content, encoding='utf-8')
    print(f'200 sites, {len(build()["entries"])} domain rules; generated files {"verified" if args.check else "updated"}')


if __name__ == '__main__':
    main()

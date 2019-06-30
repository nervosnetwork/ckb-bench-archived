# -*- coding: utf-8 -*-
# Description: CKB netdata python.d module
# SPDX-License-Identifier: GPL-3.0-or-later
#

import json

from bases.FrameworkServices.UrlService import UrlService

# default module values (can be overridden per job in `config`)
update_every = 5
priority = 60000
retries = 720

# default job configuration (overridden by python.d.plugin)
# config = {'local': {
#     'update_every': update_every,
#     'retries': retries,
#     'priority': priority,
#     'url': 'http://127.0.0.1:8114'
# }}

ORDER = ['tip', 'txpool']

CHARTS = {
    'tip': {
        'options': [None, 'Blockchain Tip', 'count', 'ckb', 'ckb.tip', 'line'],
        'lines': [
            ['tip_number', 'Tip Number', 'relative'],
            ['tip_uncles_count', 'Uncles Count', 'absolute'],
        ]
    },
    'txpool': {
        'options': [None, 'Transaction Pool', 'count', 'ckb', 'ckb.txpool', 'area'],
        'lines': [
            ['txpool_orphan', 'Orphan', 'absolute', None, None],
            ['txpool_pending', 'Pending', 'absolute', None, None],
            ['txpool_proposed', 'Proposed', 'absolute', None, None],
            ['txpool_total_tx_cycles', 'Total Cycles', 'absolute', None, 1000*1000],
            ['txpool_total_tx_size', 'Total Size', 'absolute', None, 1000],
        ],
    },
}

METHODS = {
    'get_tip_header': lambda r: {
        'tip_number': r['number'],
        'tip_uncles_count': r['uncles_count'],
    },
    'tx_pool_info': lambda r: {
        'txpool_orphan': r['orphan'],
        'txpool_pending': r['pending'],
        'txpool_proposed': r['proposed'],
        'txpool_total_tx_cycles': r['total_tx_cycles'],
        'txpool_total_tx_size': r['total_tx_size'],
    },
}

JSON_RPC_VERSION = '2.0'

class Service(UrlService):
    def __init__(self, configuration=None, name=None):
        UrlService.__init__(self, configuration=configuration, name=name)
        self.url = self.configuration.get('url', 'http://127.0.0.1:8114')
        self.header = {
            'Content-Type': 'application/json',
        }
        self.order = ORDER
        self.definitions = CHARTS

    def _get_data(self):
        data = dict()
        manager = self._manager
        for i, method in enumerate(METHODS):
            body = {
                'jsonrpc': JSON_RPC_VERSION,
                'id': i,
                'method': method,
                'params': [],
            }
            response = manager.request(method='POST',
                            url=self.url,
                            body=json.dumps(body),
                            timeout=10,
                            retries=2,
                            headers=manager.headers)

            result = json.loads(response.data.decode('utf-8'))['result']

            data.update(METHODS[method](result))

        return data

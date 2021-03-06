#!/usr/bin/env python
# -*- coding: utf-8 -*-
# Copyright (c) The Swiboe development team. All rights reserved.
# Licensed under the Apache License, Version 2.0. See LICENSE.txt
# in the project root for license information.

from ctypes import byref, c_char_p
import argparse
import json
import os
import subprocess
import sys
import threading
import unittest

import swiboe


class TestSwiboeClientLowLevel(unittest.TestCase):
    SHARED_LIBRARY = None
    TEST_SERVER = None

    def _checked_connect(self):
        client = swiboe.PtrClient()
        result = self.library.swiboe_connect(self.socket_name, byref(client))
        self._ok(result)
        return client

    def _ok(self, result):
        self.assertEqual(swiboe.SUCCESS, result)

    def setUp(self):
        self.library = swiboe.SwiboeLibrary(self.SHARED_LIBRARY)
        self.server_process = subprocess.Popen(
            [self.TEST_SERVER],
            stdout=subprocess.PIPE)
        self.test_dir = self.server_process.stdout.readline().strip()
        self.socket_name = os.path.join(self.test_dir, '_socket')

    def tearDown(self):
        self.server_process.kill()

    def test_connect_with_invalid_socket(self):
        client = swiboe.PtrClient()
        result = self.library.swiboe_connect("foobarblub", byref(client))
        self.assertEqual(result, swiboe.ERR_IO)

    def test_connect_and_disconnect(self):
        client = self._checked_connect()
        self._ok(self.library.swiboe_disconnect(client))

    def test_new_rpc(self):
        client = self._checked_connect()

        def callback(server_context, args_string):
            return

        rpc_callback = swiboe.RPC(callback)
        self._ok(self.library.swiboe_new_rpc(
            client, 'list_rust_files', 100, rpc_callback))
        self._ok(self.library.swiboe_disconnect(client))

    def test_call_not_handled_rpc(self):
        serving_client = self._checked_connect()

        def callback(server_context, args_string):
            call_result = self.library.swiboe_rpc_not_handled()
            self._ok(self.library.swiboe_server_context_finish(
                server_context, call_result))

        rpc_callback = swiboe.RPC(callback)
        self._ok(self.library.swiboe_new_rpc(
            serving_client, 'test.test', 100, rpc_callback))

        client = self._checked_connect()
        client_context = swiboe.PtrClientContext()
        self._ok(self.library.swiboe_client_call_rpc(
            client, 'test.test', 'null', byref(client_context)))
        call_result = swiboe.PtrRpcResult()
        self._ok(self.library.swiboe_client_context_wait(
            client_context, byref(call_result)))
        self.assertFalse(self.library.swiboe_rpc_result_is_ok(call_result))

        self._ok(self.library.swiboe_disconnect(client))
        self._ok(self.library.swiboe_disconnect(serving_client))

    def test_call_rpc_returning_error(self):
        serving_client = self._checked_connect()
        error = {'details': u'Needed föö, got blah.'}

        def callback(server_context, args_string):
            self.assertEqual(
                False,
                self.library.swiboe_server_context_cancelled(server_context))
            call_result = self.library.swiboe_rpc_error(
                swiboe.RPC_ERR_INVALID_ARGS, json.dumps(error))
            self._ok(self.library.swiboe_server_context_finish(
                server_context, call_result))

        rpc_callback = swiboe.RPC(callback)
        self._ok(self.library.swiboe_new_rpc(
            serving_client, 'test.test', 100, rpc_callback))

        client = self._checked_connect()
        client_context = swiboe.PtrClientContext()
        self._ok(self.library.swiboe_client_call_rpc(
            client, 'test.test', 'null', byref(client_context)))
        call_result = swiboe.PtrRpcResult()
        self.assertEqual(
            False, self.library.swiboe_client_context_done(client_context))
        self._ok(self.library.swiboe_client_context_wait(
            client_context, byref(call_result)))
        self.assertFalse(self.library.swiboe_rpc_result_is_ok(call_result))

        details = c_char_p()
        rpc_error_code = self.library.swiboe_rpc_result_unwrap_err(
            call_result, byref(details))
        self.assertEqual(swiboe.RPC_ERR_INVALID_ARGS, rpc_error_code)
        self.assertEqual(error, json.loads(details.value))
        self.library.swiboe_delete_string(details)

        self._ok(self.library.swiboe_disconnect(client))
        self._ok(self.library.swiboe_disconnect(serving_client))

    def test_call_successfull_rpc(self):
        serving_client = self._checked_connect()
        golden_return = {'blub': 'foo'}

        def callback(server_context, args_string):
            call_result = self.library.swiboe_rpc_ok(json.dumps(golden_return))
            self._ok(self.library.swiboe_server_context_finish(
                server_context, call_result))

        rpc_callback = swiboe.RPC(callback)
        self._ok(self.library.swiboe_new_rpc(
            serving_client, 'test.test', 100, rpc_callback))

        client = self._checked_connect()
        client_context = swiboe.PtrClientContext()
        self._ok(self.library.swiboe_client_call_rpc(
            client, 'test.test', 'null', byref(client_context)))
        call_result = swiboe.PtrRpcResult()

        # We do not expect any data to be streamed, so try_recv should return nothing.
        try_recv_result = c_char_p()
        self._ok(self.library.swiboe_client_context_try_recv(
            client_context, byref(try_recv_result)))
        self.assertEqual(None, try_recv_result.value)
        self.library.swiboe_delete_string(try_recv_result)

        self._ok(self.library.swiboe_client_context_wait(
            client_context, byref(call_result)))
        self.assertTrue(self.library.swiboe_rpc_result_is_ok(call_result))

        json_blob = c_char_p()
        self.library.swiboe_rpc_result_unwrap(call_result, byref(json_blob))
        self.assertEqual(golden_return, json.loads(json_blob.value))
        self.library.swiboe_delete_string(json_blob)

        self._ok(self.library.swiboe_disconnect(client))
        self._ok(self.library.swiboe_disconnect(serving_client))

    def test_call_successfull_rpc_from_inside_rpc(self):
        serving_client1 = self._checked_connect()
        golden_return = {'blub': 'foo'}

        def callback1(server_context, args_string):
            call_result = self.library.swiboe_rpc_ok(json.dumps(golden_return))
            self._ok(self.library.swiboe_server_context_finish(
                server_context, call_result))

        rpc_callback1 = swiboe.RPC(callback1)
        self._ok(self.library.swiboe_new_rpc(
            serving_client1, 'test.test', 100, rpc_callback1))

        serving_client2 = self._checked_connect()

        def callback2(server_context, args_string):
            client_context = swiboe.PtrClientContext()
            self._ok(self.library.swiboe_server_context_call_rpc(
                server_context, 'test.test', args_string, byref(
                    client_context)))
            call_result = swiboe.PtrRpcResult()
            self._ok(self.library.swiboe_client_context_wait(
                client_context, byref(call_result)))
            self._ok(self.library.swiboe_server_context_finish(
                server_context, call_result))

        rpc_callback2 = swiboe.RPC(callback2)
        self._ok(self.library.swiboe_new_rpc(
            serving_client2, 'test.foo', 100, rpc_callback2))

        client = self._checked_connect()
        client_context = swiboe.PtrClientContext()
        self._ok(self.library.swiboe_client_call_rpc(
            client, 'test.foo', 'null', byref(client_context)))
        call_result = swiboe.PtrRpcResult()
        self._ok(self.library.swiboe_client_context_wait(
            client_context, byref(call_result)))
        self.assertTrue(self.library.swiboe_rpc_result_is_ok(call_result))

        json_blob = c_char_p()
        self.library.swiboe_rpc_result_unwrap(call_result, byref(json_blob))
        self.assertEqual(golden_return, json.loads(json_blob.value))
        self.library.swiboe_delete_string(json_blob)

        self._ok(self.library.swiboe_disconnect(client))
        self._ok(self.library.swiboe_disconnect(serving_client1))
        self._ok(self.library.swiboe_disconnect(serving_client2))

    def test_streaming_successfull_rpc(self):
        serving_client = self._checked_connect()

        last = {'blub': 'foo'}

        def callback(server_context, args_string):
            for i in range(1, 4):
                update = {'count': i}
                self._ok(self.library.swiboe_server_context_update(
                    server_context, json.dumps(update)))
            call_result = self.library.swiboe_rpc_ok(json.dumps(last))
            self._ok(self.library.swiboe_server_context_finish(
                server_context, call_result))

        rpc_callback = swiboe.RPC(callback)
        self._ok(self.library.swiboe_new_rpc(
            serving_client, 'test.test', 100, rpc_callback))

        client = self._checked_connect()
        client_context = swiboe.PtrClientContext()
        self._ok(self.library.swiboe_client_call_rpc(
            client, 'test.test', 'null', byref(client_context)))

        json_str = c_char_p()
        self._ok(self.library.swiboe_client_context_recv(
            client_context, byref(json_str)))

        self.assertEqual(1, json.loads(json_str.value)['count'])
        self.library.swiboe_delete_string(json_str)

        json_str = c_char_p()
        self._ok(self.library.swiboe_client_context_recv(
            client_context, byref(json_str)))
        self.assertEqual(2, json.loads(json_str.value)['count'])
        self.library.swiboe_delete_string(json_str)

        json_str = c_char_p()
        self._ok(self.library.swiboe_client_context_recv(
            client_context, byref(json_str)))
        self.assertEqual(3, json.loads(json_str.value)['count'])
        self.library.swiboe_delete_string(json_str)

        json_str = c_char_p()
        self._ok(self.library.swiboe_client_context_recv(
            client_context, byref(json_str)))
        self.assertEqual(None, json_str.value)
        self.library.swiboe_delete_string(json_str)

        self.assertEqual(
            True, self.library.swiboe_client_context_done(client_context))

        call_result = swiboe.PtrRpcResult()
        self._ok(self.library.swiboe_client_context_wait(
            client_context, byref(call_result)))
        self.assertTrue(self.library.swiboe_rpc_result_is_ok(call_result))

        json_blob = c_char_p()
        self.library.swiboe_rpc_result_unwrap(call_result, byref(json_blob))
        self.assertEqual(last, json.loads(json_blob.value))
        self.library.swiboe_delete_string(json_blob)

        self._ok(self.library.swiboe_disconnect(client))
        self._ok(self.library.swiboe_disconnect(serving_client))

    def test_streaming_cancelled_rpc(self):
        serving_client = self._checked_connect()

        done_event = threading.Event()

        def callback(server_context, args_string):
            i = 0
            while not self.library.swiboe_server_context_cancelled(
                server_context):
                update = {'count': i}
                rv = self.library.swiboe_server_context_update(
                    server_context, json.dumps(update))
                self.assertTrue(rv == swiboe.SUCCESS or rv ==
                                swiboe.ERR_RPC_DONE)
                i += 1
            done_event.set()

        rpc_callback = swiboe.RPC(callback)
        self._ok(self.library.swiboe_new_rpc(
            serving_client, 'test.test', 100, rpc_callback))

        client = self._checked_connect()
        client_context = swiboe.PtrClientContext()
        self._ok(self.library.swiboe_client_call_rpc(
            client, 'test.test', 'null', byref(client_context)))

        for i in range(10):
            json_str = c_char_p()
            self._ok(self.library.swiboe_client_context_recv(
                client_context, byref(json_str)))

            self.assertEqual(i, json.loads(json_str.value)['count'])
            self.library.swiboe_delete_string(json_str)

        self._ok(self.library.swiboe_client_context_cancel(client_context))

        done_event.wait()
        self._ok(self.library.swiboe_disconnect(client))
        self._ok(self.library.swiboe_disconnect(serving_client))


def flatten_test_suite(suite):
    flatten = unittest.TestSuite()
    for test in suite:
        if isinstance(test, unittest.TestSuite):
            flatten.addTests(flatten_test_suite(test))
        else:
            flatten.addTest(test)
    return flatten


def parse_args():
    p = argparse.ArgumentParser(
        description=
        'Test runner for the python interface. Also tests the full C ABI.')

    p.add_argument(
        '--shared_library',
        type=str,
        default=None,
        help='The shared library that will be loaded by ctypes for testing.')
    p.add_argument('--test_server',
                   type=str,
                   default='../target/debug/test_server',
                   help='The test server binary.')
    p.add_argument('-v',
                   '--verbose',
                   action='store_true',
                   default=False,
                   help='print name of tests as they are executed')

    return p.parse_args()


def main():
    args = parse_args()

    all_test_suites = unittest.defaultTestLoader.discover(start_dir='.')
    suite = unittest.TestSuite()

    tests = set()
    for test in flatten_test_suite(all_test_suites):
        test.SHARED_LIBRARY = args.shared_library
        test.TEST_SERVER = args.test_server
        tests.add(test)
    suite.addTests(tests)
    successful = unittest.TextTestRunner(
        verbosity=0 if not args.verbose else 3,
        failfast=False).run(suite).wasSuccessful()
    return 0 if successful else 1


if __name__ == '__main__':
    sys.exit(main())

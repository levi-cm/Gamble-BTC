"""Offline contracts for the coexistence runner; never starts either miner."""
import importlib.util
import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('coexist', ROOT / 'scripts/bench-coexist.py')
coexist = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(coexist)


class CoexistContracts(unittest.TestCase):
    def test_randomx_config_has_four_workers_and_no_host_or_network_side_effects(self):
        config = coexist.randomx_config('/tmp/synthetic-xmrig.log')
        self.assertEqual(config['cpu']['rx'], [-1] * 4)
        self.assertFalse(config['randomx']['rdmsr'])
        self.assertFalse(config['randomx']['wrmsr'])
        self.assertFalse(config['randomx']['cache_qos'])
        self.assertFalse(config['autosave'])
        self.assertFalse(config['benchmark']['submit'])
        self.assertEqual(config['pools'], [])
        self.assertIsNone(config['cpu']['priority'])

    def test_hashrate_uses_benchmark_counts_and_covered_time(self):
        log = ('[2026-10-19 09:50:33.859]  bench     0.36% 36000/10000000 (30.002s)\n'
               '[2026-10-19 09:51:03.861]  bench     0.72% 72000/10000000 (60.002s)\n')
        rows = coexist.parse_progress(log)
        result = coexist.progress_rate(rows, rows[0]['wall_time'], rows[-1]['wall_time'])
        self.assertAlmostEqual(result['h_s'], 1200)
        self.assertAlmostEqual(result['covered_seconds'], 30)
        self.assertIsNone(coexist.progress_rate(rows, rows[-1]['wall_time'] + 1, float('inf'))['h_s'])

    def test_unknown_or_ambiguous_cases_are_rejected(self):
        valid = {'label': 'candidate', 'image': 'synthetic:only', 'kernel': 'pre3', 'local': 64}
        self.assertEqual(coexist.validate_cases([valid])[0]['label'], 'candidate')
        with self.assertRaises(ValueError):
            coexist.validate_cases([valid, valid])
        with self.assertRaises(ValueError):
            coexist.validate_cases([{**valid, 'label': '../escape'}])
        with self.assertRaises(ValueError):
            coexist.validate_cases([{**valid, 'wait_mode': 'spin'}])


if __name__ == '__main__':
    unittest.main()

import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch, Mock
from slayer_r9t import gpu_diagnostics as diag

class GpuEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        root=Path(self.temp.name); self.drm=root/'drm'; self.drm.mkdir()
        self.pci=root/'pci'; self.pci.mkdir(); self.proc=root/'proc'; self.proc.mkdir()
        self.nvidia=self.proc/'driver/nvidia/gpus'; self.nvidia.mkdir(parents=True)
        for key,value in [('DRM',self.drm),('PCI',self.pci),('PROC',self.proc),('NVIDIA',self.nvidia)]:
            p=patch.object(diag,key,value); p.start(); self.addCleanup(p.stop)
    def gpu(self,card='card7',address='0000:02:00.0',vendor='0x10de',minor=3):
        device=self.pci/address; device.mkdir(); (device/'vendor').write_text(vendor)
        (device/'power').mkdir()
        for key,value in {'runtime_status':'active','control':'auto','runtime_active_time':'12000','runtime_suspended_time':'3000'}.items():
            (device/'power'/key).write_text(value)
        node=self.drm/card; node.mkdir(); (node/'device').symlink_to(device)
        node=self.drm/'renderD130'; node.mkdir(); (node/'device').symlink_to(device)
        output=self.drm/(card+'-eDP-1'); output.mkdir()
        (output/'status').write_text('connected'); (output/'enabled').write_text('enabled')
        n=self.nvidia/address; n.mkdir(); (n/'information').write_text(f'Device Minor: {minor}\n')
        (n/'power').write_text('Runtime D3 status: Enabled (fine-grained)\nVideo Memory: Active\nS0ix Power Management:\n Status: Disabled\nNotebook Dynamic Boost: Supported\n')
        return device,output
    def process(self,pid=42,nodes=('/dev/nvidia3',),name='worker'):
        entry=self.proc/str(pid); entry.mkdir(); (entry/'fd').mkdir()
        (entry/'stat').write_text(f'{pid} (name with ) parentheses) S '+' '.join(['0']*18+['123']))
        (entry/'comm').write_text(name)
        for index,node in enumerate(nodes): (entry/'fd'/str(index)).symlink_to(node)
        return entry
    def test_gpu_mapping_not_hardcoded_and_shared_not_attributed(self):
        self.gpu(); self.process(nodes=('/dev/nvidia3','/dev/nvidiactl','/dev/dri/renderD130'))
        self.process(43,('/dev/nvidiactl',))
        state=diag.snapshot(); card=state['cards'][0]
        self.assertEqual(card['card'],'card7'); self.assertEqual([u['pid'] for u in card['users']],[42])
        self.assertEqual(card['users'][0]['nodes'],['/dev/dri/renderD130','/dev/nvidia3'])
        self.assertEqual(len(state['shared_users']),2)
        self.assertIn('Etkin ekran',card['evidence'][0]); self.assertFalse(state['wake_effect_verified'])
        self.assertEqual(card['driver_power']['Notebook Dynamic Boost'],'Supported')
    def test_connected_disabled_not_an_active_display(self):
        _,output=self.gpu(); (output/'enabled').write_text('disabled')
        state=diag.snapshot(); self.assertFalse(any('Etkin ekran' in s for s in state['cards'][0]['evidence']))
    def test_policy_on_separate_from_actual_runtime(self):
        device,output=self.gpu(); (device/'power/control').write_text('on')
        (device/'power/runtime_status').write_text('suspended')
        state=diag.snapshot(); card=state['cards'][0]
        self.assertEqual(card['power_after']['runtime_status'],'suspended')
        self.assertTrue(any('otomatik askıya alma kapalı' in s for s in card['evidence']))
        self.assertIn('İki okumada',card['observation'])
    def test_missing_and_malformed_power_are_absent(self):
        device,_=self.gpu(); (device/'power/runtime_status').unlink()
        (device/'power/runtime_suspended_time').write_text('bad')
        state=diag.snapshot(); power=state['cards'][0]['power_after']
        self.assertIsNone(power['runtime_status']); self.assertIsNone(power['runtime_suspended_time'])
    def test_pci_sibling_is_reported_separately(self):
        self.gpu(); sibling=self.pci/'0000:02:00.1'; sibling.mkdir(); (sibling/'power').mkdir()
        (sibling/'power/control').write_text('on'); (sibling/'power/runtime_status').write_text('active')
        sibling_state=diag.snapshot()['cards'][0]['siblings'][0]
        self.assertEqual(sibling_state['address'],'0000:02:00.1'); self.assertEqual(sibling_state['control'],'on')
    def test_permission_denied_does_not_mean_empty_gpu(self):
        self.gpu(); self.process()
        original=diag.os.scandir
        def scan(path):
            if Path(path)==self.proc/'42/fd': raise PermissionError('denied')
            return original(path)
        with patch.object(diag.os,'scandir',side_effect=scan):
            state=diag.snapshot()
        self.assertEqual(state['coverage']['permission_denied'],1); self.assertEqual(state['cards'][0]['users'],[])
    def test_pid_reuse_skips_mismatched_identity(self):
        self.process()
        with patch.object(diag,'process_start',side_effect=['123','124']):
            users,coverage=diag.device_users({'/dev/nvidia3'},self.proc)
        self.assertEqual(users,[]); self.assertEqual(coverage['vanished'],1)
    def test_scan_deadline_is_visible(self):
        self.process()
        with patch.object(diag.time,'monotonic',side_effect=[0,2]):
            users,coverage=diag.device_users({'/dev/nvidia3'},self.proc,budget=1)
        self.assertTrue(coverage['timed_out']); self.assertEqual(users,[])
    def test_fd_limit_and_name_controls(self):
        self.process(nodes=('/dev/nvidia3',)*514,name='a\x01b')
        users,coverage=diag.device_users({'/dev/nvidia3'},self.proc)
        self.assertEqual(coverage['fd_truncated'],1); self.assertEqual(users[0]['name'],'ab')
    def test_no_gpu_no_nvidia_shared_scan(self):
        with patch.object(diag,'device_users') as scan:
            state=diag.snapshot()
        scan.assert_not_called(); self.assertEqual(state['cards'],[])
    def test_does_not_open_device_nodes_or_run_commands(self):
        self.gpu(); self.process(); original=Path.open
        def opened(path,*args,**kwargs):
            self.assertFalse(str(path).startswith('/dev/'))
            if args: self.assertNotIn('w',args[0])
            return original(path,*args,**kwargs)
        with patch.object(Path,'open',opened),patch('subprocess.run') as commands:
            diag.snapshot()
        commands.assert_not_called()

class GuiGpuDiagnosticTests(unittest.TestCase):
    def test_diagnostic_poll_skips_regular_nvidia_and_service_queries(self):
        os.environ.setdefault('QT_QPA_PLATFORM','offscreen')
        from PySide6.QtWidgets import QApplication
        from slayer_r9t.gui import ControlCenter
        from slayer_r9t.settings import Settings
        app=QApplication.instance() or QApplication([]); original=ControlCenter.poll
        with tempfile.TemporaryDirectory() as temp,patch.object(ControlCenter,'poll'),patch('slayer_r9t.gui.Settings',return_value=Settings(Path(temp)/'settings.json')):
            window=ControlCenter()
            try:
                window.tabs.setCurrentIndex(window.gpu_diagnostic_index)
                with patch.object(window,'poll_gpu_diagnostics') as diag_poll,patch.object(window,'poll_gpu') as gpu,patch.object(window,'command') as rpc,patch.object(window,'desktop_command') as desktop:
                    original(window)
                diag_poll.assert_called_once(); gpu.assert_not_called(); rpc.assert_not_called(); desktop.assert_not_called()
            finally:window.close()

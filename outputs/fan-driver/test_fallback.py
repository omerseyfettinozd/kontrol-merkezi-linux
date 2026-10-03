"""Compile the real bridge against an in-memory EC and workqueue fault harness.

No module is loaded and no hardware is accessed. Run with python test_fallback.py.
"""
from pathlib import Path
import subprocess
import tempfile
import unittest


STUBS = r'''
#include <stdio.h>
#include <stdint.h>
#include <stdbool.h>
#include <string.h>
#include <errno.h>
#include <stdlib.h>
typedef uint8_t u8;
typedef uint16_t u16;
typedef uint32_t u32;
struct work_struct {int unused;};
struct delayed_work {struct work_struct work;};
struct kernel_param {int unused;};
struct kernel_param_ops {int (*set)(const char *,const struct kernel_param *);int (*get)(char *,const struct kernel_param *);};
struct notifier_block {int (*notifier_call)(struct notifier_block *,unsigned long,void *);};
static unsigned long jiffies;
static int queued, pwm_writes, reads, writes, fail_read, fail_write, fail_auto, stuck_bit, logs;
static u8 ec[0x2000];
#define DEFINE_MUTEX(x) int x
#define DECLARE_DELAYED_WORK(x,f) struct delayed_work x
#define mutex_lock(x) ((void)(x))
#define mutex_unlock(x) ((void)(x))
#define system_wq NULL
#define msecs_to_jiffies(x) (x)
#define jiffies_to_msecs(x) (x)
#define time_after_eq(a,b) ((long)((a)-(b))>=0)
#define time_before(a,b) ((long)((a)-(b))<0)
#define mod_delayed_work(q,w,t) (queued=1)
#define cancel_delayed_work_sync(w) (queued=0)
#define sysfs_streq(a,b) (!strcmp(a,b))
#define scnprintf snprintf
#define PAGE_SIZE 4096
#define PM_SUSPEND_PREPARE 1
#define PM_HIBERNATION_PREPARE 2
#define PM_RESTORE_PREPARE 3
#define PM_POST_SUSPEND 4
#define PM_POST_HIBERNATION 5
#define PM_POST_RESTORE 6
#define NOTIFY_BAD 0x8002
#define NOTIFY_OK 1
#define DMI_SYS_VENDOR 1
#define DMI_PRODUCT_NAME 2
#define dmi_match(a,b) 1
#define register_pm_notifier(x) 0
#define unregister_pm_notifier(x) ((void)(x))
#define pr_err(...) (logs++)
#define msleep(x) ((void)(x))
#define __init
#define __exit
#define module_param_cb(...)
#define module_init(...)
#define module_exit(...)
#define MODULE_LICENSE(...)
#define MODULE_DESCRIPTION(...)
#define MODULE_SOFTDEP(...)
int uniwill_read_ec_ram(u16 addr,u8 *out){reads++;if(fail_read)return -EAGAIN;*out=ec[addr];return 0;}
int uniwill_write_ec_ram(u16 addr,u8 value){writes++;if(addr==0x1804||addr==0x1809)pwm_writes++;if(fail_write)return -EIO;if(!(stuck_bit&&addr==0x0751))ec[addr]=value;return 0;}
u32 uw_set_fan_auto(void){return (u32)fail_auto;}
'''
HARNESS = r'''
#define CHECK(x) do{if(!(x)){fprintf(stderr,"line %d failed: %s\n",__LINE__,#x);return 1;}}while(0)
static void reset(void){mode=0;auto_pending=false;suspended=false;stopping=false;thermal_override=false;last_error=0;jiffies=0;deadline=15000;queued=0;pwm_writes=0;reads=0;writes=0;fail_read=0;fail_write=0;fail_auto=0;stuck_bit=0;logs=0;memset(ec,0,sizeof(ec));ec[0x0751]=0x40;ec[0x043e]=50;ec[0x044f]=50;}
static void tick(void){queued=0;worker(NULL);}
int main(void){
 char status[PAGE_SIZE];
 /* Failed lease restoration retains retry ownership; successful retry clears it. */
 reset();mode=1;jiffies=15000;fail_read=1;tick();
 CHECK(mode==1&&auto_pending&&queued&&last_error==-EAGAIN&&pwm_writes==0);
 jiffies=16000;tick();CHECK(queued&&pwm_writes==0);
 fail_read=0;tick();CHECK(mode==0&&!auto_pending&&!queued&&!(ec[0x0751]&0x40));
 /* Explicit auto failure from an idle state also owns a retry. */
 reset();fail_write=1;CHECK(control_set("auto",NULL)==-EIO);
 CHECK(auto_pending&&queued&&last_error==-EIO);
 CHECK(control_set("manual 70 70",NULL)==-EBUSY);
 fail_write=0;tick();CHECK(!auto_pending&&!queued);
 /* Upstream error and silent EC refusal both remain visible. */
 reset();mode=2;fail_auto=-EIO;CHECK(automatic()==-EIO);CHECK(auto_pending&&mode==2);
 fail_auto=0;stuck_bit=1;CHECK(automatic()==-EIO);CHECK(auto_pending);
 stuck_bit=0;CHECK(automatic()==0);CHECK(!auto_pending&&mode==0);
 /* An update fault immediately switches to restoration, never future manual writes. */
 reset();mode=1;fail_read=1;tick();CHECK(auto_pending&&queued&&pwm_writes==0);
 fail_read=0;tick();CHECK(mode==0&&!queued);
 /* A new manual lease that fails its first PWM write must queue fallback too. */
 reset();fail_write=1;CHECK(control_set("manual 70 80",NULL)==-EIO);
 CHECK(auto_pending&&queued&&mode==1);
 fail_write=0;tick();CHECK(mode==0&&!auto_pending&&!queued);
 /* Preserve the existing strict userland status key contract. */
 reset();CHECK(status_get(status,NULL)>0);CHECK(!strstr(status,"auto_pending="));
 CHECK(strstr(status,"thermal=")&&strstr(status,"error="));
 /* Successful suspend quiesces worker and rejects controls until resume. */
 reset();mode=1;queued=1;CHECK(sleep_event(NULL,PM_SUSPEND_PREPARE,NULL)==NOTIFY_OK);
 CHECK(suspended&&!queued&&mode==0);CHECK(control_set("boost",NULL)==-EBUSY);
 CHECK(sleep_event(NULL,PM_POST_SUSPEND,NULL)==NOTIFY_OK);CHECK(!suspended);
 /* Failed suspend is vetoed while restoration retries continue. */
 reset();mode=1;fail_read=1;CHECK(sleep_event(NULL,PM_SUSPEND_PREPARE,NULL)==NOTIFY_BAD);
 CHECK(!suspended&&auto_pending&&queued&&logs==1);
 fail_read=0;tick();CHECK(!auto_pending&&!queued);
 /* Exit tries finitely, reports failure, and leaves no work queued. */
 reset();mode=1;queued=1;fail_read=1;fan_exit();
 CHECK(stopping&&!queued&&reads==3&&logs==1);
 tick();CHECK(!queued&&reads==3);
 CHECK(control_set("auto",NULL)==-EBUSY);
 reset();mode=1;queued=1;fan_exit();CHECK(stopping&&!queued&&mode==0&&logs==0);
 puts("fan fallback fault harness: all scenarios passed");return 0;
}
'''


class FanBridgeFallbackTests(unittest.TestCase):
    def test_real_driver_faults_and_lifecycle(self):
        source = Path(__file__).with_name('r9t_fan.c')
        with tempfile.TemporaryDirectory(prefix='r9t-fan-test-') as folder:
            root = Path(folder)
            (root / 'linux').mkdir()
            (root / 'stubs.h').write_text(STUBS)
            for name in ('module', 'dmi', 'kernel', 'workqueue', 'mutex', 'suspend', 'delay'):
                (root / 'linux' / f'{name}.h').write_text('#include "../stubs.h"\n' if name == 'module' else '')
            harness = root / 'harness.c'
            harness.write_text(f'#include "{source.resolve()}"\n' + HARNESS)
            binary = root / 'harness'
            subprocess.run(['cc', '-std=c11', '-Wall', '-Wextra', '-Wno-unused-parameter', '-Wno-unused-variable', '-Wno-unused-function', '-I', str(root), str(harness), '-o', str(binary)], check=True)
            subprocess.run([str(binary)], check=True)


if __name__ == '__main__':
    unittest.main()

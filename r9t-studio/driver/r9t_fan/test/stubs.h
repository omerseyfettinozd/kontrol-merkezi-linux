/* Userspace stand-ins for the kernel APIs r9t_fan.c uses: an in-memory EC with
 * injectable faults and a fake workqueue. Used by crates/r9t-hal/tests/r9t_fan_c_faults.rs. */
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
#define MODULE_VERSION(...)
int uniwill_read_ec_ram(u16 addr,u8 *out){reads++;if(fail_read)return -EAGAIN;*out=ec[addr];return 0;}
int uniwill_write_ec_ram(u16 addr,u8 value){writes++;if(addr==0x1804||addr==0x1809)pwm_writes++;if(fail_write)return -EIO;if(!(stuck_bit&&addr==0x0751))ec[addr]=value;return 0;}
u32 uw_set_fan_auto(void){return (u32)fail_auto;}

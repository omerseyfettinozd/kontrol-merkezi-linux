// SPDX-License-Identifier: GPL-2.0
/* Restricted R9T fan bridge. All writes are fixed fan registers only. */
#include <linux/module.h>
#include <linux/dmi.h>
#include <linux/kernel.h>
#include <linux/workqueue.h>
#include <linux/mutex.h>
#include <linux/suspend.h>
#include <linux/delay.h>
extern int uniwill_read_ec_ram(u16,u8 *);
extern int uniwill_write_ec_ram(u16,u8);
extern u32 uw_set_fan_auto(void);
static DEFINE_MUTEX(lock);
static int mode; /* 0 auto, 1 manual, 2 full */
static unsigned int targets[2]={100,100};
static unsigned long deadline;
static int last_error;
static bool thermal_override;
static bool auto_pending;
static bool suspended;
static bool stopping;
static void worker(struct work_struct *);
static DECLARE_DELAYED_WORK(work,worker);
static int read_reg(u16 addr,u8 *value){return uniwill_read_ec_ram(addr,value);}
/* Keep the lease state until the EC confirms firmware control. A failed
 * fallback must never resume manual PWM writes or lose its retry worker. */
static int automatic(void){
 u8 reg;int ret;
 auto_pending=true;
 ret=(int)uw_set_fan_auto();
 if(!ret)ret=read_reg(0x0751,&reg);
 if(!ret)ret=uniwill_write_ec_ram(0x0751,reg&~0x40);
 if(!ret)ret=read_reg(0x0751,&reg);
 if(!ret&&(reg&0x40))ret=-EIO;
 if(ret){last_error=ret;return ret;}
 mode=0;thermal_override=false;auto_pending=false;
 return 0;
}
static void queue_retry(void){
 if(!stopping&&!suspended&&(mode||auto_pending))
  mod_delayed_work(system_wq,&work,msecs_to_jiffies(1000));
}
static int temperatures(u8 *cpu,u8 *gpu){
 int ret=read_reg(0x043e,cpu);
 if(!ret)ret=read_reg(0x044f,gpu);
 if(!ret&&(*cpu<1||*cpu>115||*gpu<1||*gpu>115))ret=-ERANGE;
 return ret;
}
static int update(void){
 u8 reg,cpu,gpu;int ret;
 ret=temperatures(&cpu,&gpu);
 if(ret)return ret;
 thermal_override=cpu>=95||gpu>=87;
 ret=read_reg(0x0751,&reg);
 if(ret)return ret;
 if(!(reg&0x40))return -ECANCELED; /* Firmware/hotkey changed the mode. */
 if(mode){
  ret=uniwill_write_ec_ram(0x1804,(mode==2||thermal_override)?200:targets[0]*2);
  if(!ret)ret=uniwill_write_ec_ram(0x1809,(mode==2||thermal_override)?200:targets[1]*2);
 }
 return ret;
}
static void worker(struct work_struct *unused){
 int ret=0;
 mutex_lock(&lock);
 if(stopping||suspended)goto out;
 if(auto_pending)automatic();
 else if(mode){
  if(time_after_eq(jiffies,deadline)){last_error=-ETIMEDOUT;automatic();}
  else{
   ret=update();
   if(ret){last_error=ret;automatic();}
  }
 }
 queue_retry();
out:
 mutex_unlock(&lock);
}
static int control_set(const char *value,const struct kernel_param *kp){
 unsigned int cpu=100,gpu=100;int wanted=0,ret=0;u8 reg,t0,t1;char extra;
 if(sysfs_streq(value,"auto"))wanted=0;
 else if(sysfs_streq(value,"boost"))wanted=2;
 else if(sscanf(value,"manual %u %u %c",&cpu,&gpu,&extra)==2&&cpu>=50&&cpu<=100&&gpu>=50&&gpu<=100)wanted=1;
 else return -EINVAL;
 mutex_lock(&lock);
 if(stopping||suspended){ret=-EBUSY;goto out;}
 if(!wanted){ret=automatic();queue_retry();goto out;}
 if(auto_pending){ret=-EBUSY;goto out;}
 ret=temperatures(&t0,&t1);
 if(ret)goto out;
 ret=read_reg(0x0751,&reg);
 if(ret)goto out;
 if(!(reg&0x40))ret=uniwill_write_ec_ram(0x0751,reg|0x40);
 if(ret)goto out;
 mode=wanted;targets[0]=cpu;targets[1]=gpu;deadline=jiffies+msecs_to_jiffies(15000);last_error=0;
 ret=update();
 if(ret){last_error=ret;automatic();}
 queue_retry();
out:
 mutex_unlock(&lock);
 return ret;
}
static int rpm(u16 addr,int *value){
 u8 high,low,check;int ret,i;
 for(i=0;i<3;i++){
  ret=read_reg(addr,&high);if(ret)return ret;
  ret=read_reg(addr+1,&low);if(ret)return ret;
  ret=read_reg(addr,&check);if(ret)return ret;
  if(high==check){*value=(high<<8)|low;return *value<=12000?0:-ERANGE;}
 }
 return -EAGAIN;
}
static int status_get(char *buffer,const struct kernel_param *kp){
 u8 cpu,gpu,reg,pwm0,pwm1;int r0,r1,ret,remaining=0;
 mutex_lock(&lock);
 ret=temperatures(&cpu,&gpu);
 if(!ret)ret=rpm(0x0464,&r0);
 if(!ret)ret=rpm(0x046c,&r1);
 if(!ret)ret=read_reg(0x0751,&reg);
 if(!ret)ret=read_reg(0x1804,&pwm0);
 if(!ret)ret=read_reg(0x1809,&pwm1);
 if(!ret){
  if(mode&&time_before(jiffies,deadline))remaining=jiffies_to_msecs(deadline-jiffies);
  ret=scnprintf(buffer,PAGE_SIZE,"mode=%d cpu_target=%u gpu_target=%u cpu_rpm=%d gpu_rpm=%d cpu_temp=%u gpu_temp=%u mode_byte=%u cpu_pwm=%u gpu_pwm=%u lease_ms=%d error=%d thermal=%d\n",mode,targets[0],targets[1],r0,r1,cpu,gpu,reg,pwm0,pwm1,remaining,last_error,thermal_override);
 }
 mutex_unlock(&lock);
 return ret;
}
static const struct kernel_param_ops control_ops={.set=control_set};
static const struct kernel_param_ops status_ops={.get=status_get};
module_param_cb(control,&control_ops,NULL,0200);
module_param_cb(status,&status_ops,NULL,0400);
static int sleep_event(struct notifier_block *nb,unsigned long event,void *data){
 int ret=0;
 if(event==PM_SUSPEND_PREPARE||event==PM_HIBERNATION_PREPARE||event==PM_RESTORE_PREPARE){
  /* Block controls and requeues before waiting for a running worker. */
  mutex_lock(&lock);suspended=true;mutex_unlock(&lock);
  cancel_delayed_work_sync(&work);
  mutex_lock(&lock);
  if(mode||auto_pending)ret=automatic();
  if(ret){suspended=false;queue_retry();}
  mutex_unlock(&lock);
  if(ret){pr_err("r9t_fan: automatic fan restore failed before sleep: %d\n",ret);return NOTIFY_BAD;}
 }else if(event==PM_POST_SUSPEND||event==PM_POST_HIBERNATION||event==PM_POST_RESTORE){
  mutex_lock(&lock);suspended=false;queue_retry();mutex_unlock(&lock);
 }
 return NOTIFY_OK;
}
static struct notifier_block sleep_notifier={.notifier_call=sleep_event};
static int __init fan_init(void){
 u8 project;int ret;
 if(!dmi_match(DMI_SYS_VENDOR,"GAME GARAJ")||!dmi_match(DMI_PRODUCT_NAME,"SLAYER R9T"))return -ENODEV;
 ret=read_reg(0x0740,&project);
 if(ret||project!=0x1a)return -ENODEV;
 return register_pm_notifier(&sleep_notifier);
}
static void __exit fan_exit(void){
 int ret=0,i;
 unregister_pm_notifier(&sleep_notifier);
 mutex_lock(&lock);stopping=true;mutex_unlock(&lock);
 cancel_delayed_work_sync(&work);
 mutex_lock(&lock);
 for(i=0;(mode||auto_pending)&&i<3;i++){
  ret=automatic();if(!ret)break;
  if(i<2)msleep(100);
 }
 mutex_unlock(&lock);
 if(ret)pr_err("r9t_fan: automatic fan restore failed on unload: %d; firmware control is unconfirmed\n",ret);
}
module_init(fan_init);module_exit(fan_exit);
MODULE_LICENSE("GPL");
MODULE_DESCRIPTION("Slayer R9T fan control with RPM feedback and 15-second lease");
MODULE_SOFTDEP("pre: uniwill_wmi tuxedo_keyboard");

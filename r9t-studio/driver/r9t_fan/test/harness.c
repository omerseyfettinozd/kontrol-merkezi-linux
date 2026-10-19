/* Fault and lifecycle scenarios for the real r9t_fan.c (included before this file).
 * No module is loaded and no hardware is accessed. */
#define CHECK(x) do{if(!(x)){fprintf(stderr,"line %d failed: %s\n",__LINE__,#x);return 1;}}while(0)
static void reset(void){mode=0;auto_pending=false;suspended=false;stopping=false;thermal_override=false;last_error=0;jiffies=0;deadline=15000;queued=0;pwm_writes=0;reads=0;writes=0;fail_read=0;fail_write=0;fail_auto=0;stuck_bit=0;logs=0;memset(ec,0,sizeof(ec));ec[0x0751]=0x40;ec[0x043e]=50;ec[0x044f]=50;
 targets[0]=targets[1]=100;allow_low_duty=false;min_duty=50;cache_ms=1000;held[0]=held[1]=0;stall_ticks=0;stalls=0;override_last=0;memset(&snap,0,sizeof(snap));snap_gen=0;rpm_value[0]=rpm_value[1]=3200;fail_hwmon=0;}
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
 /* 0.3.0: status2 carries the v1 fields first, then the new ones. */
 reset();CHECK(status2_get(status,NULL)>0);
 CHECK(!strncmp(status,"mode=0 cpu_target=100",21)&&strstr(status," thermal=0 sample_age_ms=")&&strstr(status,"min_duty=50")&&strstr(status,"stalls=0\n"));
 /* Readers share one EC refresh per cache_ms; control writes invalidate it. */
 reset();CHECK(status_get(status,NULL)>0);{int r=reads;
  CHECK(status_get(status,NULL)>0&&reads==r);
  jiffies=999;CHECK(status_get(status,NULL)>0&&reads==r);
  jiffies=1000;CHECK(status_get(status,NULL)>0&&reads>r);r=reads;
  CHECK(control_set("manual 60 60",NULL)==0);r=reads;
  CHECK(status_get(status,NULL)>0&&reads>r&&strstr(status,"mode=1 ")&&strstr(status,"cpu_pwm=120"));}
 /* A failed refresh is reported, not served from a stale cache. */
 reset();CHECK(status_get(status,NULL)>0);jiffies=5000;fail_read=1;CHECK(status_get(status,NULL)==-EAGAIN);fail_read=0;
 CHECK(cache_ms_set("100",NULL)==-EINVAL&&cache_ms_set("2000",NULL)==0&&cache_ms==2000);
 /* Defaults keep 0.2.0 commands: below 50 % is refused and the floor never raises a 50+ command. */
 reset();CHECK(control_set("manual 49 60",NULL)==-EINVAL);
 ec[0x043e]=80;ec[0x044f]=75;CHECK(control_set("manual 50 60",NULL)==0&&ec[0x1804]==100&&ec[0x1809]==120);
 /* Override engages at 95/87 °C and releases below 90/82 °C after a 10 s hold. */
 reset();CHECK(control_set("manual 60 60",NULL)==0);
 ec[0x043e]=96;jiffies=1000;deadline=60000;tick();CHECK(thermal_override&&ec[0x1804]==200);
 ec[0x043e]=89;jiffies=3000;tick();CHECK(thermal_override);
 ec[0x043e]=91;jiffies=11500;tick();CHECK(thermal_override);
 ec[0x043e]=89;ec[0x044f]=82;jiffies=12000;tick();CHECK(thermal_override);
 ec[0x044f]=81;jiffies=12500;tick();CHECK(!thermal_override&&ec[0x1804]==120);
 ec[0x044f]=87;jiffies=13000;tick();CHECK(thermal_override&&ec[0x1809]==200);
 /* min_duty is the measured floor: 25–50, below it manual is refused. */
 reset();CHECK(min_duty_set("24",NULL)==-EINVAL&&min_duty_set("51",NULL)==-EINVAL&&min_duty_set("30",NULL)==0);
 CHECK(control_set("manual 29 60",NULL)==-EINVAL);CHECK(control_set("manual 30 30",NULL)==0&&ec[0x1804]==60&&ec[0x1809]==60);
 /* ...and rises to 50 % with temperature, with 3 °C hysteresis on the way down. */
 ec[0x043e]=75;ec[0x044f]=62;jiffies=1000;deadline=60000;tick();CHECK(ec[0x1804]==100&&ec[0x1809]==78);
 ec[0x043e]=73;jiffies=2000;tick();CHECK(ec[0x1804]==100);
 ec[0x043e]=72;jiffies=3000;tick();CHECK(ec[0x1804]==92);
 /* allow_low_duty opens 20 %; a fan under 500 rpm for two ticks goes back to firmware. */
 reset();CHECK(control_set("manual 20 60",NULL)==-EINVAL);CHECK(allow_low_duty_set("1",NULL)==0);
 CHECK(control_set("manual 20 60",NULL)==0&&ec[0x1804]==40&&ec[0x1809]==120);
 rpm_value[0]=400;deadline=60000;jiffies=1000;tick();CHECK(mode==1&&stall_ticks==1);
 rpm_value[0]=900;jiffies=2000;tick();CHECK(mode==1&&stall_ticks==0);
 rpm_value[0]=300;jiffies=3000;tick();jiffies=4000;tick();
 CHECK(mode==0&&stalls==1&&last_error==-EPIPE&&!(ec[0x0751]&0x40));
 CHECK(status2_get(status,NULL)>0&&strstr(status,"error=-32")&&strstr(status,"stalls=1")&&strstr(status,"allow_low_duty=1"));
 /* The test base ramps from 20 % at 70 °C to 50 % at 80 °C (CPU). */
 reset();allow_low_duty_set("1",NULL);ec[0x043e]=75;CHECK(control_set("manual 20 20",NULL)==0&&ec[0x1804]==70&&ec[0x1809]==40);
 ec[0x043e]=80;deadline=60000;jiffies=1000;tick();CHECK(ec[0x1804]==100);
 /* Turning allow_low_duty off lifts an active low command to min_duty on the next tick. */
 reset();allow_low_duty_set("1",NULL);CHECK(control_set("manual 20 20",NULL)==0);
 allow_low_duty_set("0",NULL);deadline=60000;jiffies=1000;tick();CHECK(ec[0x1804]==100&&ec[0x1809]==100&&mode==1);
 /* hwmon reads the cache: RPM, millidegrees and 0–255 duty. */
 reset();ec[0x043e]=66;ec[0x044f]=61;ec[0x1804]=200;ec[0x1809]=100;rpm_value[1]=3511;
 {long v;const char *s;
  CHECK(hw_read(NULL,hwmon_fan,hwmon_fan_input,1,&v)==0&&v==3511);
  CHECK(hw_read(NULL,hwmon_temp,hwmon_temp_input,0,&v)==0&&v==66000);
  CHECK(hw_read(NULL,hwmon_pwm,hwmon_pwm_input,0,&v)==0&&v==255);
  CHECK(hw_read(NULL,hwmon_pwm,hwmon_pwm_input,1,&v)==0&&v==127);
  CHECK(hw_label(NULL,hwmon_fan,hwmon_fan_label,1,&s)==0&&!strcmp(s,"GPU"));
  {int r=reads;CHECK(hw_read(NULL,hwmon_fan,hwmon_fan_input,0,&v)==0&&reads==r);}}
 /* hwmon failure does not stop the module from loading. */
 reset();fail_hwmon=1;ec[0x0740]=0x1a;CHECK(fan_init()==0&&hwmon==NULL&&logs==1);hw_unregister();
 puts("fan fallback fault harness: all scenarios passed");return 0;
}

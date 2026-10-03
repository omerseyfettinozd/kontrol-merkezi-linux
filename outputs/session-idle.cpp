// SPDX-License-Identifier: MIT
#include <QGuiApplication>
#include <KIdleTime>
#include <cstdio>
int main(int argc,char **argv){
 QGuiApplication app(argc,argv);
 bool ok=false;const int seconds=argc==2?QString::fromLocal8Bit(argv[1]).toInt(&ok):0;
 if(!ok||seconds<1||seconds>3600)return 2;
 auto idle=KIdleTime::instance();const int identifier=idle->addIdleTimeout(seconds*1000);
 QObject::connect(idle,&KIdleTime::timeoutReached,[idle,identifier](int token,int){
  if(token!=identifier)return;std::puts("idle");std::fflush(stdout);idle->catchNextResumeEvent();
 });
 QObject::connect(idle,&KIdleTime::resumingFromIdle,[]{std::puts("active");std::fflush(stdout);});
 return app.exec();
}

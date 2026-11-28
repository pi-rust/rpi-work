package com.pirust.rpiwork

import android.app.Application
import com.google.mlkit.common.MlKit

class WorkMobileApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        MlKit.initialize(this)
    }
}

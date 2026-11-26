node('linux') {
    stage('Release') {
        def version = input message: 'Release which version?', ok: 'Release',
            parameters: [string(name: 'VERSION', defaultValue: '1.0.0')]
        sh "./release.sh '${version}'"
    }
}

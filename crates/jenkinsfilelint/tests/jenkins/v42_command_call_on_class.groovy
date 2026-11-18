pipeline {
    agent any
    stages {
        stage('A') {
            steps {
                script {
                    Utils.notify currentBuild, status: 'ok'
                    Utils.log msg, 'info'
                    Utils.deploy env, version, dryRun: true
                }
            }
        }
    }
}

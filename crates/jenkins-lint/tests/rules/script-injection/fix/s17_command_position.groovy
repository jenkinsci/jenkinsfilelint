pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                withEnv(["CMD=make ${env.CHANGE_TITLE}"]) {
                    sh "${CMD}"
                    sh "cd src && ${CMD}"
                }
            }
        }
    }
}

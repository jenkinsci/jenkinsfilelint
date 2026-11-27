pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                sh "git checkout ${env.CHANGE_BRANCH}"
            }
        }
    }
}

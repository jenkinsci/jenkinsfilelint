pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                sh "echo ${CHANGE_TITLE} ${currentBuild.number}"
            }
        }
    }
}

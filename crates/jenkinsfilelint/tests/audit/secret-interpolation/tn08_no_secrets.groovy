pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                sh "echo ${env.BRANCH_NAME} ${BUILD_NUMBER}"
                withCredentials([string(credentialsId: 'token', variable: 'TOKEN')]) {
                    sh "echo ${env.BRANCH_NAME}"
                }
            }
        }
    }
}

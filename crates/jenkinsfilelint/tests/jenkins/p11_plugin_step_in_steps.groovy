pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                junit 'reports/*.xml'
                archiveArtifacts artifacts: 'build/**'
            }
        }
    }
}

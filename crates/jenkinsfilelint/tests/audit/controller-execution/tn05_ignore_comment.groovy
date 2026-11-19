pipeline {
    // jenkinsfilelint: ignore[controller-execution] this job backs up JENKINS_HOME
    agent { label 'built-in' }
    stages {
        stage('Backup') {
            steps {
                sh 'tar czf /backup/jenkins.tgz "$JENKINS_HOME"'
            }
        }
    }
}

node('built-in') { // jenkinsfilelint: ignore[controller-execution]
    sh 'df -h'
}

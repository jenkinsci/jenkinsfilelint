pipeline {
    agent 'any'
    stages {
        stage('Build') {
            steps {
                echo 'x'
            }
        }
    }
    post {
        success("JOB SUCCESS") {
            echo 'done'
        }
    }
}

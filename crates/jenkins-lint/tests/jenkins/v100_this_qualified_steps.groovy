pipeline {
    agent any
    stages {
        stage('Build') {
            steps {
                this.echo 'one'
                this.('echo') 'two'
                this.(echo) 'three'
                this.sh script: 'make'
                this.retry(2) {
                    echo 'again'
                }
                this.script {
                    echo 'four'
                }
            }
            post {
                always {
                    this.echo 'done'
                }
            }
        }
    }
}

pipeline {
    this.agent any
    this.options {
        this.timeout(time: 1, unit: 'HOURS')
    }
    stages {
        this.stage('Build') {
            when {
                this.branch 'main'
            }
            this.steps {
                echo 'x'
            }
        }
    }
}
